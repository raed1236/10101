use crate::commons::reqwest_client;
use anyhow::bail;
use anyhow::Result;
use commons::NewOrder;
use commons::OrderResponse;
use reqwest::Url;

pub struct OrderbookClient {
    url: Url,
}

impl OrderbookClient {
    pub fn new(url: Url) -> Self {
        Self { url }
    }

    pub(crate) async fn post_new_order(&self, order: NewOrder) -> Result<OrderResponse> {
        let url = self.url.join("/api/orderbook/orders")?;
        let client = reqwest_client();

        let response = client.post(url).json(&order).send().await?;

        if response.status().as_u16() == 200 {
            let response = response.json().await?;
            Ok(response)
        } else {
            tracing::error!("Could not create new order");
            bail!("Could not create new order: {response:?}")
        }
    }
}
