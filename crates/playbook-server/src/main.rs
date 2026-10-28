fn main() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    eprintln!("playbook status on {address}");
    playbook_server::serve_once(listener, &playbook_server::demo_snapshot()).expect("serve");
}
