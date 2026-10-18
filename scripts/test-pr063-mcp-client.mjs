import assert from "node:assert/strict";

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";

const endpoint = process.env.EEZ_MCP_SMOKE_URL;
const token = process.env.EEZ_MCP_SMOKE_TOKEN;

assert.ok(endpoint, "EEZ_MCP_SMOKE_URL must point at the local Worker");
assert.ok(token, "EEZ_MCP_SMOKE_TOKEN must contain the throwaway local token");

const client = new Client({ name: "eezfunnel-mcp-interop-smoke", version: "1.0.0" });
const transport = new StreamableHTTPClientTransport(new URL(endpoint), {
  requestInit: {
    headers: { Authorization: `Bearer ${token}` },
  },
});

await client.connect(transport);
try {
  const { tools } = await client.listTools();
  assert.ok(tools.some(({ name }) => name === "list_commands"), "list_commands is available");

  const result = await client.callTool({ name: "list_commands", arguments: {} });
  assert.equal(result.isError, false, "list_commands succeeds through the official client");
  const text = result.content.find((item) => item.type === "text")?.text;
  assert.ok(text, "list_commands returns a text result");
  const commandInventory = JSON.parse(text);
  assert.ok(Array.isArray(commandInventory.commands), "command inventory is an array");
  assert.ok(commandInventory.commands.length > 0, "command inventory is not empty");

  console.log("PR-063 official MCP TypeScript client: initialize, tools/list, list_commands passed");
} finally {
  await client.close();
}
