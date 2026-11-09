pub fn existing() -> anyhow::Result<()> {
    let y = 1;
    run().context(format!("old {y}"))?;
    Ok(())
}
