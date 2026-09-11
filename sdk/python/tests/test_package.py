import smabar_sdk


def test_version_is_exposed() -> None:
    assert smabar_sdk.__version__ == "1.0.0"
