use alloy::providers::{Provider, RootProvider};

#[tokio::test]
async fn https_provider_can_fetch_chain_id() {
    let provider: RootProvider = RootProvider::builder()
        .connect("https://ethereum-rpc.publicnode.com")
        .await
        .expect("connect to HTTPS Ethereum RPC endpoint");

    assert_eq!(provider.get_chain_id().await.unwrap(), 1);
}
