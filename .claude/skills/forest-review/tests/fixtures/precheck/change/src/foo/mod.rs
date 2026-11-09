pub fn existing() -> anyhow::Result<()> {
    let y = 1;
    run().context(format!("old {y}"))?;
    Ok(())
}

pub fn added() -> anyhow::Result<()> {
    let y = 2;
    run().context(format!("new {y}"))?;
    // This matches Lotus behaviour for null rounds.
    // https://github.com/filecoin-project/lotus/blob/master/chain/store/store.go#L10
    // https://github.com/filecoin-project/lotus/blob/v1.38.3/chain/store/store.go#L10
    // https://github.com/filecoin-project/lotus/blob/0123456789abcdef0123456789abcdef01234567/chain/store/store.go#L10
    // Retry once — the peer may be slow.
    let s = "a — b";
    let _ = fil_actor_miner_state::v16::State::default;
    let _ = std::env::var("FOREST_NEW_KNOB");
    let _ = std::env::var("FOREST_FOO_HEIGHT");
    let _ = std::env::var("FOREST_DOCUMENTED_KNOB");
    std::thread::sleep(std::time::Duration::from_millis(1));
    assert!(err().to_string().contains("boom"));
    Ok(())
}

#[allow(clippy::indexing_slicing)]
fn first(v: &[u8]) -> u8 {
    v[0]
}

fn decode_header(buf: &[u8]) -> Vec<u8> {
    let len = buf.len();
    let mut out = Vec::with_capacity(len);
    let fixed: Vec<u8> = Vec::with_capacity(16);
    let n = len as u32;
    let z = vec![0u8; len];
    out.extend(fixed);
    out.push(n as u8 + z.len() as u8);
    out
}

fn render(len: usize) -> Vec<u8> {
    Vec::with_capacity(len)
}

impl RpcMethod for Foo {
    const PERMISSION: Permission = Permission::Read;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let msg = "{ unbalanced in string";
        let c = '{';
        run().context(format!("in test {}", 1)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let e = err();
        assert!(e.to_string().contains("boom"));
        let _ = fil_actor_miner_state::v16::State::default;
        let _ = std::env::var("FOREST_TEST_ONLY");
    }
}

fn after_tests() {
    std::thread::sleep(std::time::Duration::from_millis(1));
}

fn wrap(e: anyhow::Error) -> anyhow::Result<()> {
    // Same as Lotus:
    // https://github.com/filecoin-project/lotus/blob/v1.38.3/chain/store/store.go#L10
    Err(e.context(format!("wrapped {}", 1)))
}

#[test]
fn t1() -> Result<(), String> {
    std::thread::sleep(std::time::Duration::from_millis(1));
    Ok(())
}

#[cfg(test)]
fn helper<A, B>(a: A, b: B) {
    std::thread::sleep(std::time::Duration::from_millis(1));
    let _ = std::env::var("FOREST_IN_HELPER");
}

#[cfg(test)]
impl<A: Clone, B> Foo<A, B> {
    fn g() {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

fn after_generics() {
    std::thread::sleep(std::time::Duration::from_millis(1));
}

#[cfg(test)]
fn v2<T>(x: T) -> bool where T: Into<Vec<u8>>, T: Copy {
    std::thread::sleep(std::time::Duration::from_millis(1));
    true
}

#[cfg(test)]
fn v3<T, U>(t: T, u: U)
where
    T: Clone,
    U: Copy,
{
    std::thread::sleep(std::time::Duration::from_millis(1));
}

fn after_where() {
    std::thread::sleep(std::time::Duration::from_millis(1));
}

fn guards(x: u64) -> anyhow::Result<()> {
    if x == 0 {
        anyhow::bail!("zero");
    }
    if x > 10 {
        bail!(
            "too big: {}",
            x
        );
    }
    if x == 5 { bail!("five") }
    if x == 7 {
        bail!("seven");
    } else {
        println!("x");
    }
    if let Some(y) = Some(x) {
        bail!("{y}");
    }
    Ok(())
}
