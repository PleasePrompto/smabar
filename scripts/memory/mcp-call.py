# One bar_ui_state action over MCP, for the inspector scripts.
# Usage: mcp-call.py <action> [tileId]
import sys
from mcp_client import Client
extra = {"tileId": sys.argv[2]} if len(sys.argv) > 2 else {}
Client().call("bar_ui_state", action=sys.argv[1], **extra)
