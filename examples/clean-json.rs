fn main() -> anyhow::Result<()> {
    for arg in std::env::args().skip(1 /* binary name */) {
        eprintln!("{}", arg);
        let mut v =
            serde_json::from_reader::<_, serde_json::Value>(std::fs::read(&arg)?.as_slice())?;
        zero_tt(&mut v);
        std::fs::write(arg, serde_json::to_string_pretty(&v)?)?;
    }
    Ok(())
}

fn zero_tt(v: &mut serde_json::Value) {
    use serde_json::Value::*;
    match v {
        Null | Bool(_) | Number(_) | String(_) => {}
        Array(a) => {
            for i in a {
                zero_tt(i)
            }
        }
        Object(o) => {
            for (k, v) in o {
                match k.as_str() {
                    "tt" => *v = Number(serde_json::Number::from(0)),
                    _ => zero_tt(v),
                }
            }
        }
    }
}
