use amt_wsman::{get_power_state, AmtConfig};

#[tokio::test]
#[ignore = "requires real AMT hardware and AMT_TEST_HOST/PORT/USER/PASS"]
async fn authenticated_power_state_read() {
    let cfg = AmtConfig {
        host: std::env::var("AMT_TEST_HOST").expect("AMT_TEST_HOST is required"),
        port: std::env::var("AMT_TEST_PORT")
            .unwrap_or_else(|_| "16993".to_string())
            .parse()
            .expect("AMT_TEST_PORT must be a TCP port"),
        username: std::env::var("AMT_TEST_USER").expect("AMT_TEST_USER is required"),
        password: std::env::var("AMT_TEST_PASS").expect("AMT_TEST_PASS is required"),
    };

    let (power_state, _) = get_power_state(&cfg)
        .await
        .expect("authenticated WS-Man power-state read failed");
    assert_ne!(power_state, 0, "firmware returned an invalid PowerState");
}
