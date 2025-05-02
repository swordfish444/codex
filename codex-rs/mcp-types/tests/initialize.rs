use mcp_types::ClientCapabilities;
use mcp_types::ClientRequest;
use mcp_types::Implementation;
use mcp_types::InitializeRequestParams;
use mcp_types::JSONRPCMessage;
use mcp_types::JSONRPCRequest;
use mcp_types::RequestId;
use serde_json::json;

#[test]
fn deserialize_initialize_request() {
    // An example `initialize` request taken from the Model-Context-Protocol
    // specification (trimmed down to the required fields so that the message
    // is still minimal yet valid).
    let raw = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "clientInfo": { "name": "acme-client", "version": "1.2.3" },
            "protocolVersion": "2025-03-26"
        }
    }"#;

    // First deserialize from the wire into a JSONRPCMessage, as would happen in
    // a real read loop.
    let msg: JSONRPCMessage =
        serde_json::from_str(raw).expect("failed to deserialize JSONRPCMessage");

    // Extract the request variant.
    let JSONRPCMessage::Request(json_req) = msg else {
        unreachable!()
    };

    let expected_req = JSONRPCRequest {
        id: RequestId::Integer(1),
        method: "initialize".into(),
        params: Some(json!({
            "capabilities": {},
            "clientInfo": { "name": "acme-client", "version": "1.2.3" },
            "protocolVersion": "2025-03-26"
        })),
    };

    assert_eq!(json_req, expected_req);

    // Convert to strongly-typed ClientRequest without conditional branching.
    let client_req: ClientRequest =
        ClientRequest::try_from(json_req).expect("conversion must succeed");

    let ClientRequest::InitializeRequest(init_params) = client_req else {
        unreachable!()
    };

    assert_eq!(
        init_params,
        InitializeRequestParams {
            capabilities: ClientCapabilities {
                experimental: None,
                roots: None,
                sampling: None,
            },
            client_info: Implementation {
                name: "acme-client".into(),
                version: "1.2.3".into(),
            },
            protocol_version: "2025-03-26".into(),
        }
    );
}
