use std::process::Command;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let out = Command::new("zig")
            .args(["ar", "t", "../../target/debug/c_api.lib"])
            .output()
            .expect("zig ar failed");

        let members: Vec<&str> = std::str::from_utf8(&out.stdout)
            .unwrap()
            .lines()
            .filter(|l| l.contains("compiler_builtins"))
            .collect();

        if !members.is_empty() {
            let mut cmd = Command::new("zig");
            cmd.args(["ar", "d", "../../target/debug/c_api.lib"]);
            cmd.args(&members);
            cmd.status().expect("zig ar d failed");
        }

        println!("Oink");
    }
}
