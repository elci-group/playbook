fn main() {
    std::process::exit(playbook_cli::run(
        &std::env::args().skip(1).collect::<Vec<_>>(),
    ));
}
