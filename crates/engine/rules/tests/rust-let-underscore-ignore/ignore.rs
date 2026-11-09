struct Socket;

fn fire_and_forget(sock: Socket) {
    // ruleid: rust/let-underscore-ignore
    let _ = send_packet(sock);
    // ruleid: rust/let-underscore-ignore
    let _ = writeln!(std::io::stdout(), "ping");
}

fn keep_value(a: i32) -> i32 {
    // 具名绑定不报
    let _unused = compute(a);
    let _ = a;
    compute(a)
}
