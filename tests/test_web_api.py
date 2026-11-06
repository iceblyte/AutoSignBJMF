"""Web 后端接口测试：Flask test_client + 临时 SQLite，全程不发真实网络请求。

覆盖：健康检查、静态页、任务创建/查询/日志、用户配置接口，以及管理员
初始化 / 登录 / token 鉴权 / 修改 / 删除。
"""

import json
from datetime import datetime, timedelta

import pytest

from conftest import WEB_SIGNIN_DIR  # noqa: F401  # 确保 web_signin 在 sys.path 上

import app as web_app
import app.db as web_db
from app.models import Task, TaskLog
from app.qr_login import QRSessionState


@pytest.fixture
def flask_app(tmp_path, monkeypatch):
    monkeypatch.setattr(web_db, "DB_PATH", tmp_path / "api.db")
    application = web_app.create_app()
    application.config.update(TESTING=True)
    yield application
    scheduler = application.config.get("SCHEDULER")
    if scheduler and scheduler.running:
        scheduler.shutdown(wait=False)


@pytest.fixture
def client(flask_app):
    return flask_app.test_client()


def _state(token="t1", cookie="remember_student_h=TOKEN", name="甲", class_id="123456"):
    return QRSessionState(
        token=token,
        created_at=0,
        cookie=cookie,
        user_name=name,
        class_id=class_id,
    )


def _patch_qr(monkeypatch, state):
    monkeypatch.setattr(
        web_app.qr_service,
        "get_session_state",
        lambda token: state if token == state.token else None,
    )


def _add_task(application, *, cookie="remember_student_h=TOKEN", name="甲", times=None, created_at=None):
    SessionLocal = application.config["DB_SESSION_FACTORY"]
    with SessionLocal() as db:
        task = Task(
            name=name,
            class_id="123456",
            cookie=cookie,
            lat="30.1",
            lng="120.2",
            acc="30",
            wx_key="",
            qq_key="",
            times=json.dumps(times or ["07:30:00"]),
            enabled=True,
            created_at=created_at or datetime.utcnow(),
        )
        db.add(task)
        db.commit()
        db.refresh(task)
        return task.id


def _init_and_login(client, key="s3cret"):
    assert client.post("/api/admin/init", json={"key": key}).status_code == 200
    resp = client.post("/api/admin/login", json={"key": key})
    assert resp.status_code == 200
    return resp.get_json()["token"]


# ========== 基础 ==========


def test_health(client):
    resp = client.get("/health")

    assert resp.status_code == 200
    assert resp.get_json() == {"status": "ok"}


def test_index_and_manage_pages_served(client):
    assert client.get("/").status_code == 200
    assert client.get("/manage").status_code == 200


def test_unknown_task_returns_404(client):
    assert client.get("/api/tasks/999").status_code == 404
    assert client.get("/api/tasks/999/logs").status_code == 404


# ========== 任务创建 ==========


def test_create_task_requires_token(client):
    resp = client.post("/api/tasks", json={})

    assert resp.status_code == 400
    assert resp.get_json()["error"] == "missing token"


def test_create_task_rejects_unknown_token(client):
    resp = client.post("/api/tasks", json={"token": "nope"})

    assert resp.status_code == 400
    assert resp.get_json()["error"] == "invalid or expired token"


def test_create_task_requires_coord_and_times(client, monkeypatch):
    _patch_qr(monkeypatch, _state())

    resp = client.post("/api/tasks", json={"token": "t1", "times": ["07:30:00"]})

    assert resp.status_code == 400
    assert "经纬度" in resp.get_json()["error"]


def test_create_task_rejects_bad_date(client, monkeypatch):
    _patch_qr(monkeypatch, _state())

    resp = client.post(
        "/api/tasks",
        json={"token": "t1", "coord": "30.1 120.2", "times": ["07:30"], "date_start": "2026/01/01"},
    )

    assert resp.status_code == 400
    assert "日期格式错误" in resp.get_json()["error"]


def test_create_task_parses_coord_and_hides_cookie(client, monkeypatch):
    _patch_qr(monkeypatch, _state())

    resp = client.post(
        "/api/tasks",
        json={
            "token": "t1",
            "coord": "30.1，120.2",
            "times": ["07:30:00"],
            "date_start": "2026-01-01",
            "date_end": "2026-01-31",
        },
    )

    assert resp.status_code == 201
    data = resp.get_json()
    assert data["lat"] == "30.1"
    assert data["lng"] == "120.2"
    assert data["name"] == "甲"
    assert data["class_id"] == "123456"
    assert "cookie" not in data
    assert data["date_end"].endswith("23:59:59")

    detail = client.get(f"/api/tasks/{data['id']}").get_json()
    assert detail["times"] == ["07:30:00"]
    assert "cookie" not in detail


# ========== 日志与用户侧接口 ==========


def test_task_logs_returned_latest_first(client, flask_app):
    task_id = _add_task(flask_app)
    SessionLocal = flask_app.config["DB_SESSION_FACTORY"]
    with SessionLocal() as db:
        db.add(TaskLog(task_id=task_id, status="success", message="旧", run_at=datetime(2026, 1, 1, 1, 0, 0)))
        db.add(TaskLog(task_id=task_id, status="error", message="新", run_at=datetime(2026, 1, 2, 1, 0, 0)))
        db.commit()

    resp = client.get(f"/api/tasks/{task_id}/logs")

    assert resp.status_code == 200
    assert [item["message"] for item in resp.get_json()] == ["新", "旧"]


def test_user_config_returns_latest_task(client, monkeypatch, flask_app):
    cookie = "remember_student_h=TOKEN"
    _add_task(flask_app, cookie=cookie, times=["07:30:00"], created_at=datetime(2026, 1, 1))
    _add_task(flask_app, cookie=cookie, times=["21:31:00"], created_at=datetime(2026, 1, 2))
    _patch_qr(monkeypatch, _state(cookie=cookie))

    data = client.get("/api/user/config?token=t1").get_json()

    assert data["has_task"] is True
    assert data["cookie_changed"] is False
    assert data["config"]["times"] == ["21:31:00"]


def test_user_config_without_task(client, monkeypatch):
    _patch_qr(monkeypatch, _state(cookie="remember_student_none=OTHER"))

    data = client.get("/api/user/config?token=t1").get_json()

    assert data == {"has_task": False, "cookie_changed": False, "config": None}


def test_user_endpoints_require_valid_token(client):
    assert client.get("/api/user/config").status_code == 400
    assert client.get("/api/user/config?token=bad").status_code == 400
    assert client.get("/api/user/logs").status_code == 400


def test_user_logs_are_scoped_to_own_cookie(client, monkeypatch, flask_app):
    mine = _add_task(flask_app, cookie="remember_student_h=MINE", name="我")
    other = _add_task(flask_app, cookie="remember_student_h=OTHER", name="别人")
    SessionLocal = flask_app.config["DB_SESSION_FACTORY"]
    with SessionLocal() as db:
        db.add(TaskLog(task_id=mine, status="success", message="我的日志", run_at=datetime(2026, 1, 1)))
        db.add(TaskLog(task_id=other, status="success", message="别人的日志", run_at=datetime(2026, 1, 2)))
        db.commit()
    _patch_qr(monkeypatch, _state(cookie="remember_student_h=MINE"))

    logs = client.get("/api/user/logs?token=t1").get_json()

    assert [log["message"] for log in logs] == ["我的日志"]


# ========== 管理员鉴权 ==========


def test_admin_status_is_public_and_initially_false(client):
    resp = client.get("/api/admin/status")

    assert resp.status_code == 200
    assert resp.get_json() == {"initialized": False}


def test_admin_endpoints_require_initialized_key(client):
    resp = client.get("/api/admin/tasks")

    assert resp.status_code == 401
    assert resp.get_json()["need_init"] is True


def test_admin_login_before_init_returns_400(client):
    assert client.post("/api/admin/login", json={"key": "x"}).status_code == 400


def test_admin_init_only_once(client):
    client.post("/api/admin/init", json={"key": "a"})

    resp = client.post("/api/admin/init", json={"key": "b"})

    assert resp.status_code == 409
    assert client.get("/api/admin/status").get_json() == {"initialized": True}


def test_admin_init_requires_key(client):
    assert client.post("/api/admin/init", json={}).status_code == 400


def test_admin_login_rejects_wrong_key(client):
    client.post("/api/admin/init", json={"key": "right"})

    assert client.post("/api/admin/login", json={"key": "wrong"}).status_code == 401


def test_admin_endpoints_reject_missing_or_bad_token(client):
    token = _init_and_login(client)

    assert client.get("/api/admin/tasks").status_code == 401
    assert client.get("/api/admin/tasks", headers={"X-Admin-Token": "forged"}).status_code == 401
    assert client.get("/api/admin/tasks", headers={"X-Admin-Token": token}).status_code == 200


def test_admin_token_accepted_via_query_param(client, flask_app):
    token = _init_and_login(client)
    _add_task(flask_app)

    resp = client.get(f"/api/admin/tasks?admin_token={token}")

    assert resp.status_code == 200
    assert len(resp.get_json()) == 1


def test_expired_admin_token_is_rejected_and_purged(client, flask_app):
    token = _init_and_login(client)
    flask_app.config["ADMIN_TOKENS"][token] = datetime.utcnow() - timedelta(seconds=1)

    resp = client.get("/api/admin/tasks", headers={"X-Admin-Token": token})

    assert resp.status_code == 401
    assert resp.get_json()["error"] == "admin token expired"
    assert token not in flask_app.config["ADMIN_TOKENS"]


def test_admin_list_filters_by_name_and_enabled(client, flask_app):
    token = _init_and_login(client)
    _add_task(flask_app, name="甲")
    _add_task(flask_app, name="乙")
    headers = {"X-Admin-Token": token}

    assert len(client.get("/api/admin/tasks", headers=headers).get_json()) == 2
    assert [t["name"] for t in client.get("/api/admin/tasks?name=甲", headers=headers).get_json()] == ["甲"]


# ========== 管理员写操作 ==========


def test_admin_update_task_reschedules_jobs(client, flask_app):
    token = _init_and_login(client)
    task_id = _add_task(flask_app)
    headers = {"X-Admin-Token": token}

    resp = client.patch(
        f"/api/admin/tasks/{task_id}",
        json={"times": ["08:00:00"], "lat": "31.5", "enabled": True},
        headers=headers,
    )

    assert resp.status_code == 200
    data = resp.get_json()
    assert data["times"] == ["08:00:00"]
    assert data["lat"] == "31.5"
    job_ids = [job.id for job in flask_app.config["SCHEDULER"].get_jobs()]
    assert f"task_{task_id}_080000" in job_ids


def test_admin_disable_task_removes_jobs(client, flask_app):
    token = _init_and_login(client)
    task_id = _add_task(flask_app)
    headers = {"X-Admin-Token": token}
    client.patch(f"/api/admin/tasks/{task_id}", json={"times": ["08:00:00"]}, headers=headers)

    client.patch(f"/api/admin/tasks/{task_id}", json={"enabled": False}, headers=headers)

    assert [job.id for job in flask_app.config["SCHEDULER"].get_jobs()] == []


def test_admin_update_rejects_bad_times_and_date(client, flask_app):
    token = _init_and_login(client)
    task_id = _add_task(flask_app)
    headers = {"X-Admin-Token": token}

    assert client.patch(f"/api/admin/tasks/{task_id}", json={"times": "08:00"}, headers=headers).status_code == 400
    assert client.patch(
        f"/api/admin/tasks/{task_id}", json={"date_end": "2026/01/01"}, headers=headers
    ).status_code == 400


def test_admin_delete_task_cascades_logs(client, flask_app):
    token = _init_and_login(client)
    task_id = _add_task(flask_app)
    SessionLocal = flask_app.config["DB_SESSION_FACTORY"]
    with SessionLocal() as db:
        db.add(TaskLog(task_id=task_id, status="success", message="日志"))
        db.commit()

    resp = client.delete(f"/api/admin/tasks/{task_id}", headers={"X-Admin-Token": token})

    assert resp.status_code == 200
    with SessionLocal() as db:
        assert db.query(TaskLog).count() == 0
    assert client.get(f"/api/tasks/{task_id}").status_code == 404


def test_admin_logs_require_token_and_validate_task_id(client, flask_app):
    token = _init_and_login(client)
    task_id = _add_task(flask_app)
    headers = {"X-Admin-Token": token}
    SessionLocal = flask_app.config["DB_SESSION_FACTORY"]
    with SessionLocal() as db:
        db.add(TaskLog(task_id=task_id, status="success", message="日志"))
        db.commit()

    assert client.get("/api/admin/logs").status_code == 401
    assert len(client.get("/api/admin/logs", headers=headers).get_json()) == 1
    assert client.get("/api/admin/logs?task_id=abc", headers=headers).status_code == 400


def test_delete_unknown_task_returns_404(client):
    token = _init_and_login(client)

    assert client.delete("/api/admin/tasks/999", headers={"X-Admin-Token": token}).status_code == 404
