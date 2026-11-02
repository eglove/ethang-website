use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut out = PathBuf::from("dist");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "build" => {}
            "--out" => {
                if let Some(value) = args.next() {
                    out = PathBuf::from(value);
                }
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }
    ethang_website::pages::build_site(&out)?;
    println!("built {}", out.display());
    Ok(())
}