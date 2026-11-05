import ipaddress
import json
import os

from app import create_app

DEFAULT_PORT = 9988
# 用户侧接口（/api/qr、/api/tasks、/api/user/config）本身不做鉴权，
# 默认只监听回环地址；容器里需要对外服务时显式设置 WEB_SIGNIN_HOST=0.0.0.0。
DEFAULT_HOST = "127.0.0.1"


def _config_path() -> str:
    return os.path.join(os.path.dirname(__file__), "config.json")


def _read_config() -> dict:
    config_path = _config_path()
    if not os.path.exists(config_path):
        return {}
    try:
        with open(config_path, "r", encoding="utf-8") as f:
            data = json.load(f)
        return data if isinstance(data, dict) else {}
    except Exception:
        # 配置解析失败时回退到默认值
        return {}


def _is_bindable_host(value) -> bool:
    if not isinstance(value, str) or not value:
        return False
    if value == "localhost":
        return True
    try:
        ipaddress.ip_address(value)
    except ValueError:
        return False
    return True


def _load_port_from_config() -> int:
    """
    从配置文件 / 环境变量中加载端口号。

    优先级：
    1. 环境变量 WEB_SIGNIN_PORT
    2. 当前目录下的 config.json 中的 "port"
    3. 默认 9988，与 Docker 中暴露的端口保持一致
    """
    for candidate in (os.getenv("WEB_SIGNIN_PORT"), _read_config().get("port")):
        if candidate is None:
            continue
        try:
            port = int(candidate)
        except (TypeError, ValueError):
            continue
        if 1 <= port <= 65535:
            return port

    return DEFAULT_PORT


def _load_host_from_config() -> str:
    """
    加载监听地址，优先级与端口一致：环境变量 > config.json > 默认回环地址。
    """
    for candidate in (os.getenv("WEB_SIGNIN_HOST"), _read_config().get("host")):
        if _is_bindable_host(candidate):
            return candidate

    return DEFAULT_HOST


app = create_app()


if __name__ == "__main__":
    # 运行时根据配置文件 / 环境变量决定监听地址与端口
    app.run(host=_load_host_from_config(), port=_load_port_from_config(), debug=False)
