#![cfg(unix)]

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    thread,
};

use herdr_tree::herdr::HerdrClient;

#[test]
fn socket_client_decodes_snapshot_and_reports_metadata() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let socket = directory.path().join("herdr.sock");
    let listener = UnixListener::bind(&socket).expect("bind fake Herdr socket");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept snapshot request");
        let mut request = String::new();
        BufReader::new(stream.try_clone().expect("clone stream"))
            .read_line(&mut request)
            .expect("read snapshot request");
        assert!(request.contains("session.snapshot"));
        let response = concat!(
            "{\"id\":\"snapshot\",\"result\":{\"type\":\"session_snapshot\",\"snapshot\":{",
            "\"version\":\"0.9.1\",\"protocol\":22,\"focused_workspace_id\":\"w1\",",
            "\"focused_tab_id\":\"t1\",\"focused_pane_id\":null,",
            "\"workspaces\":[{\"workspace_id\":\"w1\",\"number\":1,\"label\":\"dashboard\",",
            "\"focused\":true,\"tab_count\":1,\"active_tab_id\":\"t1\"}],",
            "\"tabs\":[{\"tab_id\":\"t1\",\"workspace_id\":\"w1\",\"number\":4,",
            "\"label\":\"shell\",\"focused\":true,\"pane_count\":1}],",
            "\"panes\":[],\"layouts\":[],\"agents\":[]}}}\n"
        );
        stream
            .write_all(response.as_bytes())
            .expect("write snapshot response");

        let (mut stream, _) = listener.accept().expect("accept metadata request");
        let mut request = String::new();
        BufReader::new(stream.try_clone().expect("clone stream"))
            .read_line(&mut request)
            .expect("read metadata request");
        let request: serde_json::Value = serde_json::from_str(&request).expect("valid request");
        assert_eq!(request["method"], "workspace.report_metadata");
        assert_eq!(request["params"]["tokens"]["tree_workspace"], "▾ dashboard");
        stream
            .write_all(
                b"{\"id\":\"metadata\",\"result\":{\"type\":\"workspace_metadata_reported\"}}\n",
            )
            .expect("write metadata response");
    });

    let client = HerdrClient::new(socket);
    let snapshot = client.snapshot().expect("decode snapshot");
    assert_eq!(snapshot.workspaces[0].label, "dashboard");
    let tokens = BTreeMap::from([("tree_workspace".into(), Some("▾ dashboard".into()))]);
    client
        .report_metadata("w1", 42, &tokens)
        .expect("report metadata");
    server.join().expect("fake server exits cleanly");
}
