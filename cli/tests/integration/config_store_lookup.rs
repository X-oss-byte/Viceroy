use crate::common::{Test, TestResult};
use hyper::{body::to_bytes, StatusCode};

#[tokio::test(flavor = "multi_thread")]
async fn json_config_store_lookup_works() -> TestResult {
    const FASTLY_TOML: &str = r#"
        name = "json-config_store-lookup"
        description = "json config_store lookup test"
        authors = ["Jill Bryson <jbryson@fastly.com>", "Rose McDowall <rmcdowall@fastly.com>"]
        language = "rust"
        [local_server]
        [local_server.config_stores]
        [local_server.config_stores.animals]
        file = "../test-fixtures/data/json-config_store.json"
        format = "json"
    "#;

    let resp = Test::using_fixture("config_store-lookup.wasm")
        .using_fastly_toml(FASTLY_TOML)?
        .against_empty()
        .await;

    assert_eq!(resp.status(), StatusCode::OK);
    assert!(to_bytes(resp.into_body())
        .await
        .expect("can read body")
        .to_vec()
        .is_empty());

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn inline_toml_config_store_lookup_works() -> TestResult {
    const FASTLY_TOML: &str = r#"
        name = "inline-toml-config_store-lookup"
        description = "inline toml config_store lookup test"
        authors = ["Jill Bryson <jbryson@fastly.com>", "Rose McDowall <rmcdowall@fastly.com>"]
        language = "rust"
        [local_server]
        [local_server.config_stores]
        [local_server.config_stores.animals]
        format = "inline-toml"
        [local_server.config_stores.animals.contents]
        dog = "woof"
        cat = "meow"
    "#;

    let resp = Test::using_fixture("config_store-lookup.wasm")
        .using_fastly_toml(FASTLY_TOML)?
        .against_empty()
        .await;

    assert_eq!(resp.status(), StatusCode::OK);
    assert!(to_bytes(resp.into_body())
        .await
        .expect("can read body")
        .to_vec()
        .is_empty());

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_config_store_works() -> TestResult {
    const FASTLY_TOML: &str = r#"
        name = "missing-config_store-config"
        description = "missing config_store test"
        language = "rust"
    "#;

    let resp = Test::using_fixture("config_store-lookup.wasm")
        .using_fastly_toml(FASTLY_TOML)?
        .against_empty()
        .await;

    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

    Ok(())
}
