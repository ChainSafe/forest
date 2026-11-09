// Deleted — never reported.
pub fn f() -> anyhow::Result<()> {
    g().context(format!("deleted {}", 1))
}
