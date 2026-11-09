#![allow(clippy::unwrap_used)]

#[tokio::test]
async fn it_works() {
    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    let err = run().unwrap_err();
    assert!(err.to_string().contains("nope"));
    assert!(text().contains("[+] sync ok"));
}
