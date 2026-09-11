"""smabar-sdk: write a smabar plugin in a single Python file.

Minimal plugin::

    from smabar_sdk import Plugin

    app = Plugin()

    @app.every(2.0)
    def update() -> None:
        app.render("hello", "tile", "<div>Hi</div>")

    app.run()
"""

from smabar_sdk.plugin import Plugin
from smabar_sdk.protocol import RpcError, RpcTimeoutError

__version__ = "1.0.0"

__all__ = ["Plugin", "RpcError", "RpcTimeoutError", "__version__"]
