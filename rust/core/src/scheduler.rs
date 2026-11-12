//! 每日定时 + 失败重试。
//!
//! 对应 `desktop/scheduler.py` 的 `DailyScheduler`。行为要点：
//!
//! - 多个时间点：每次取**严格晚于现在**的最近一个
//! - 失败重试：单次到点最多执行 `attempts` 次（含首次），间隔 `retry_delay`
//! - 暂停：到点后**跳过**这一次（不补跑）
//! - 等待切片：长等待切成 `chunk`，保证暂停/退出能及时响应
//! - 时钟、等待函数、执行函数、日志回调全部注入，所以能脱离真实时间测试
//!
//! 线程由调用方起（`run_loop` 就是线程体），这里不自己 `spawn`，
//! 这样测试里可以直接在栈上跑。

use std::time::Duration;

use chrono::NaiveDateTime;

use crate::timepoints::{next_fire, Clock};

/// 调度过程中对外播报的事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// 算出了下一次执行时间。
    NextRun { at: NaiveDateTime },
    /// 开始第 `attempt` 次（共 `total` 次）执行。
    RunStart { attempt: u32, total: u32 },
    /// 第 `attempt` 次结束，退出码 `code`。
    RunDone { attempt: u32, total: u32, code: i32 },
    /// 失败，`delay` 之后重试。
    Retry { delay: Duration },
    /// 到点时处于暂停状态，本次跳过。
    Skipped { at: NaiveDateTime },
    Paused,
    Resumed,
    Stopped,
    /// 内部错误（读时间点失败等），不影响继续跑。
    Error { message: String },
}

/// 注入的执行环境。
pub struct Hooks<Run, Times, Now, Wait, Ev> {
    pub run: Run,
    pub times: Times,
    pub now: Now,
    pub wait: Wait,
    pub on_event: Ev,
}

/// 调度器。不持有线程，`run_loop` 是线程体。
pub struct Scheduler<Run, Times, Now, Wait, Ev>
where
    Run: FnMut() -> i32,
    Times: FnMut() -> Vec<Clock>,
    Now: FnMut() -> NaiveDateTime,
    Wait: FnMut(Duration) -> bool,
    Ev: FnMut(Event),
{
    hooks: Hooks<Run, Times, Now, Wait, Ev>,
    attempts: u32,
    retry_delay: Duration,
    chunk: Duration,
    paused: bool,
    stop: bool,
    pub next_run_at: Option<NaiveDateTime>,
    pub last_exit_code: Option<i32>,
}

impl<Run, Times, Now, Wait, Ev> Scheduler<Run, Times, Now, Wait, Ev>
where
    Run: FnMut() -> i32,
    Times: FnMut() -> Vec<Clock>,
    Now: FnMut() -> NaiveDateTime,
    Wait: FnMut(Duration) -> bool,
    Ev: FnMut(Event),
{
    pub fn new(hooks: Hooks<Run, Times, Now, Wait, Ev>) -> Self {
        Self {
            hooks,
            attempts: 3,
            retry_delay: Duration::from_secs(300),
            chunk: Duration::from_secs(60),
            paused: false,
            stop: false,
            next_run_at: None,
            last_exit_code: None,
        }
    }

    pub fn with_retry(mut self, attempts: u32, retry_delay: Duration) -> Self {
        self.attempts = attempts.max(1);
        self.retry_delay = retry_delay;
        self
    }

    pub fn with_chunk(mut self, chunk: Duration) -> Self {
        self.chunk = chunk.max(Duration::from_secs(1));
        self
    }

    pub fn pause(&mut self) {
        self.paused = true;
        (self.hooks.on_event)(Event::Paused);
    }

    pub fn resume(&mut self) {
        self.paused = false;
        (self.hooks.on_event)(Event::Resumed);
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn stop(&mut self) {
        self.stop = true;
    }

    pub fn is_stopped(&self) -> bool {
        self.stop
    }

    /// 立刻执行一次（不重试），返回退出码。界面上的「立即签到」用它。
    pub fn run_now(&mut self) -> i32 {
        let code = (self.hooks.run)();
        self.last_exit_code = Some(code);
        code
    }

    /// 等到下一个时间点。返回 `None` 表示被停止。
    pub fn wait_for_next(&mut self) -> Option<NaiveDateTime> {
        loop {
            if self.stop {
                return None;
            }
            let times = match (self.hooks.times)() {
                times if !times.is_empty() => times,
                _ => {
                    // 没有时间点：短睡一会儿再看，别空转
                    if !(self.hooks.wait)(self.chunk) {
                        return None;
                    }
                    continue;
                }
            };

            let now = (self.hooks.now)();
            let target = match next_fire(&times, now) {
                Ok(target) => target,
                Err(error) => {
                    (self.hooks.on_event)(Event::Error {
                        message: format!("计算下次执行时间失败：{error}"),
                    });
                    if !(self.hooks.wait)(self.chunk) {
                        return None;
                    }
                    continue;
                }
            };

            self.next_run_at = Some(target);
            (self.hooks.on_event)(Event::NextRun { at: target });

            if !self.wait_until(target) {
                return None;
            }
            return Some(target);
        }
    }

    /// 按次数与间隔执行一轮（含首次）。
    pub fn fire(&mut self) -> i32 {
        let mut code = 1;
        for attempt in 1..=self.attempts {
            (self.hooks.on_event)(Event::RunStart {
                attempt,
                total: self.attempts,
            });
            code = (self.hooks.run)();
            self.last_exit_code = Some(code);
            (self.hooks.on_event)(Event::RunDone {
                attempt,
                total: self.attempts,
                code,
            });

            if code == 0 || attempt >= self.attempts {
                return code;
            }
            if !self.retry_delay.is_zero() {
                (self.hooks.on_event)(Event::Retry {
                    delay: self.retry_delay,
                });
                let delay = chrono::Duration::from_std(self.retry_delay).unwrap_or_default();
                let target = (self.hooks.now)() + delay;
                if !self.wait_until(target) {
                    return code;
                }
            }
        }
        code
    }

    /// 线程体：等到点 → 暂停则跳过 → 否则执行。
    pub fn run_loop(&mut self) {
        while !self.stop {
            let Some(at) = self.wait_for_next() else {
                break;
            };
            if self.paused {
                (self.hooks.on_event)(Event::Skipped { at });
                continue;
            }
            self.fire();
        }
        self.next_run_at = None;
        (self.hooks.on_event)(Event::Stopped);
    }

    fn wait_until(&mut self, target: NaiveDateTime) -> bool {
        loop {
            if self.stop {
                return false;
            }
            let remaining = (target - (self.hooks.now)()).num_milliseconds();
            if remaining <= 0 {
                return true;
            }
            let slice = std::cmp::min(
                Duration::from_millis(remaining as u64),
                self.chunk,
            );
            if !(self.hooks.wait)(slice) {
                return false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timepoints::parse_times;
    use chrono::NaiveDate;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// 可编程的假环境：时间随时钟推进，执行结果按脚本返回。
    struct Fake {
        now: NaiveDateTime,
        slept: Vec<Duration>,
        codes: Vec<i32>,
        runs: usize,
        events: Vec<Event>,
        /// 每次 wait 被调用时把时钟推进相应秒数（模拟真实等待）
        advance: bool,
        /// 第 n 次 wait 之后返回 false，模拟被中断
        interrupt_at: Option<usize>,
        waits: usize,
    }

    impl Fake {
        fn new(now: NaiveDateTime) -> Rc<RefCell<Self>> {
            Rc::new(RefCell::new(Self {
                now,
                slept: Vec::new(),
                codes: Vec::new(),
                runs: 0,
                events: Vec::new(),
                advance: true,
                interrupt_at: None,
                waits: 0,
            }))
        }
    }

    fn at(y: i32, m: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    }

    /// 把 Fake 包装成 Scheduler 需要的五个闭包。
    macro_rules! build {
        ($fake:expr, $times:expr, $attempts:expr, $delay:expr) => {{
            let f = $fake.clone();
            let times = $times;
            let clock = f.clone();
            let waiter = f.clone();
            let runner = f.clone();
            let ev = f.clone();
            Scheduler::new(Hooks {
                run: move || {
                    let mut state = runner.borrow_mut();
                    let code = state.codes.get(state.runs).copied().unwrap_or(0);
                    state.runs += 1;
                    code
                },
                times: {
                    let times = times.clone();
                    move || times.clone()
                },
                now: move || clock.borrow().now,
                wait: move |duration: Duration| {
                    let mut state = waiter.borrow_mut();
                    state.waits += 1;
                    state.slept.push(duration);
                    if state.advance {
                        state.now += chrono::Duration::from_std(duration).unwrap();
                    }
                    if let Some(limit) = state.interrupt_at {
                        if state.waits >= limit {
                            return false;
                        }
                    }
                    true
                },
                on_event: move |event: Event| ev.borrow_mut().events.push(event),
            })
            .with_retry($attempts, $delay)
            .with_chunk(Duration::from_secs(60))
        }};
    }

    #[test]
    fn 到点执行一次后继续等下一个点() {
        let fake = Fake::new(at(2026, 11, 11, 21, 30));
        let times = parse_times("21:31").unwrap();
        let mut scheduler = build!(fake, times, 3, Duration::from_secs(300));

        let at_time = scheduler.wait_for_next().unwrap();
        assert_eq!(at_time, at(2026, 11, 11, 21, 31));
        assert_eq!(scheduler.fire(), 0);
        assert_eq!(fake.borrow().runs, 1);
    }

    #[test]
    fn 成功之后不再重试() {
        let fake = Fake::new(at(2026, 11, 11, 21, 30));
        fake.borrow_mut().codes = vec![0];
        let times = parse_times("21:31").unwrap();
        let mut scheduler = build!(fake.clone(), times, 3, Duration::from_secs(300));

        scheduler.wait_for_next();
        scheduler.fire();

        assert_eq!(fake.borrow().runs, 1);
        let starts = fake
            .borrow()
            .events
            .iter()
            .filter(|e| matches!(e, Event::RunStart { .. }))
            .count();
        assert_eq!(starts, 1);
    }

    #[test]
    fn 失败按次数与间隔重试() {
        let fake = Fake::new(at(2026, 11, 11, 21, 30));
        fake.borrow_mut().codes = vec![1, 1, 1];
        let times = parse_times("21:31").unwrap();
        let mut scheduler = build!(fake.clone(), times, 3, Duration::from_secs(120));

        scheduler.wait_for_next();
        assert_eq!(scheduler.fire(), 1);

        assert_eq!(fake.borrow().runs, 3);
        let retries = fake
            .borrow()
            .events
            .iter()
            .filter(|e| matches!(e, Event::Retry { .. }))
            .count();
        assert_eq!(retries, 2, "3 次尝试之间应该有 2 次重试");
    }

    #[test]
    fn 重试后成功就停() {
        let fake = Fake::new(at(2026, 11, 11, 21, 30));
        fake.borrow_mut().codes = vec![1, 0];
        let times = parse_times("21:31").unwrap();
        let mut scheduler = build!(fake.clone(), times, 3, Duration::from_secs(60));

        scheduler.wait_for_next();
        assert_eq!(scheduler.fire(), 0);
        assert_eq!(fake.borrow().runs, 2);
    }

    #[test]
    fn 暂停时到点跳过() {
        let fake = Fake::new(at(2026, 11, 11, 21, 30));
        let times = parse_times("21:31").unwrap();
        let mut scheduler = build!(fake.clone(), times, 3, Duration::from_secs(60));
        scheduler.pause();
        // 等到过 21:31（两次 60s 切片）之后被打断，避免死循环
        fake.borrow_mut().interrupt_at = Some(3);

        scheduler.run_loop();

        assert_eq!(fake.borrow().runs, 0, "暂停期间不应该执行");
        assert!(fake
            .borrow()
            .events
            .iter()
            .any(|e| matches!(e, Event::Skipped { .. })));
    }

    #[test]
    fn 停止后等待返回_none() {
        let fake = Fake::new(at(2026, 11, 11, 20, 0));
        let times = parse_times("21:31").unwrap();
        let mut scheduler = build!(fake, times, 3, Duration::from_secs(60));

        scheduler.stop();
        assert!(scheduler.wait_for_next().is_none());
    }

    #[test]
    fn 等待被中断时返回_none() {
        let fake = Fake::new(at(2026, 11, 11, 20, 0));
        fake.borrow_mut().interrupt_at = Some(2);
        let times = parse_times("21:31").unwrap();
        let mut scheduler = build!(fake, times, 3, Duration::from_secs(60));

        assert!(scheduler.wait_for_next().is_none());
    }

    #[test]
    fn 没有时间点时只等待不执行() {
        let fake = Fake::new(at(2026, 11, 11, 20, 0));
        fake.borrow_mut().interrupt_at = Some(3);
        let mut scheduler = build!(fake.clone(), Vec::new(), 3, Duration::from_secs(60));

        assert!(scheduler.wait_for_next().is_none());
        assert_eq!(fake.borrow().runs, 0);
        assert_eq!(fake.borrow().slept.len(), 3);
    }

    #[test]
    fn 下一次执行时间会播报出来() {
        let fake = Fake::new(at(2026, 11, 11, 20, 0));
        let times = parse_times("07:30, 21:31").unwrap();
        let mut scheduler = build!(fake.clone(), times, 3, Duration::from_secs(60));

        let target = scheduler.wait_for_next().unwrap();
        assert_eq!(target, at(2026, 11, 11, 21, 31));
        assert_eq!(scheduler.next_run_at, Some(target));
        assert!(fake
            .borrow()
            .events
            .iter()
            .any(|e| matches!(e, Event::NextRun { .. })));
    }

    #[test]
    fn 立即执行不重试() {
        let fake = Fake::new(at(2026, 11, 11, 20, 0));
        fake.borrow_mut().codes = vec![1, 0];
        let times = parse_times("21:31").unwrap();
        let mut scheduler = build!(fake.clone(), times, 3, Duration::from_secs(60));

        assert_eq!(scheduler.run_now(), 1);
        assert_eq!(fake.borrow().runs, 1);
        assert_eq!(scheduler.last_exit_code, Some(1));
    }

    #[test]
    fn 重试间隔为_0_时不等待() {
        let fake = Fake::new(at(2026, 11, 11, 21, 30));
        fake.borrow_mut().codes = vec![1, 1];
        let times = parse_times("21:31").unwrap();
        let mut scheduler = build!(fake.clone(), times, 2, Duration::from_secs(0));

        scheduler.wait_for_next();
        let before = fake.borrow().slept.len();
        scheduler.fire();
        // 只有等到点那一次等待，没有额外的重试等待
        assert_eq!(fake.borrow().slept.len(), before);
        assert_eq!(fake.borrow().runs, 2);
    }
}
