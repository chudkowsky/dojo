use futures::Future;
use jsonrpsee::{
    core::{Error, RpcResult},
    http_client::{HttpClient, HttpClientBuilder},
};
use katana_primitives::block::BlockHashOrNumber;
use katana_provider::traits::block::BlockHashProvider;
use katana_rpc_api::starknet::StarknetApiClient;
use katana_rpc_types::block::MaybePendingBlockWithTxs;

/// A RPC client for fetching data from the forked network. This client is responsible for
/// fetching block data only.
#[derive(Debug)]
pub struct ForkedClientFactory {
    /// The HTTP client.
    client: HttpClient,
    /// The forked block id.
    block_id: BlockHashOrNumber,
}

impl ForkedClientFactory {
    pub fn new(url: impl AsRef<str>, block_id: BlockHashOrNumber) -> Result<Self, Error> {
        let client = HttpClientBuilder::default().build(url)?;
        Ok(Self { client, block_id })
    }

    pub fn with_provider<'a, P>(&self, provider: P) -> ForkedClient<'a, P>
    where
        P: BlockHashProvider,
    {
        ForkedClient { provider, client: &self.client, block_id: self.block_id }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ForkedClientError {
    RpcError(#[from] Error),
}

#[derive(Debug)]
pub struct ForkedClient<'a, P> {
    provider: P,
    client: &'a HttpClient,
    block_id: BlockHashOrNumber,
}

impl<'a, P: BlockHashProvider> ForkedClient<'a, P> {
    pub async fn get_block_with_txs(
        &self,
        block_id: BlockHashOrNumber,
    ) -> Result<MaybePendingBlockWithTxs, ForkedClientError> {
        self.on_valid_block(block_id, |block| self.client.block_with_txs(block_id.into())).await
    }

    async fn on_valid_block<F, T>(
        &self,
        block_id: BlockHashOrNumber,
        func: F,
    ) -> Result<T, ForkedClientError>
    where
        F: FnOnce(BlockHashOrNumber) -> dyn Future<Output = RpcResult<T>>,
    {
        // Checks that the requested block is a valid forked block. The requested block must be have
        // a block number that is smaller or equal to the forked block number.
        func(block_id).await.map_err(ForkedClientError::from)
    }
}
