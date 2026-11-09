pub fn f() -> anyhow::Result<()> {
    g().context(format!("renamed {}", 1))
}
