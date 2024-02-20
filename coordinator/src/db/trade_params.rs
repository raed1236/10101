use crate::dlc_protocol;
use crate::orderbook::db::custom_types::Direction;
use crate::schema::trade_params;
use bitcoin::secp256k1::PublicKey;
use diesel::result::Error::RollbackTransaction;
use diesel::ExpressionMethods;
use diesel::PgConnection;
use diesel::QueryDsl;
use diesel::QueryResult;
use diesel::Queryable;
use diesel::RunQueryDsl;
use dlc_manager::ReferenceId;
use ln_dlc_node::util;
use rust_decimal::prelude::ToPrimitive;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Queryable, Debug)]
#[diesel(table_name = trade_params)]
#[allow(dead_code)] // We have to allow dead code here because diesel needs the fields to be able to derive queryable.
pub(crate) struct TradeParams {
    pub id: i32,
    pub protocol_id: Uuid,
    pub trader_pubkey: String,
    pub quantity: f32,
    pub leverage: f32,
    pub average_price: f32,
    pub direction: Direction,
}

pub(crate) fn insert(
    conn: &mut PgConnection,
    protocol_id: ReferenceId,
    params: &commons::TradeParams,
) -> QueryResult<()> {
    let protocol_id =
        util::parse_from_reference_id(protocol_id).map_err(|_| RollbackTransaction)?;
    let average_price = params
        .average_execution_price()
        .to_f32()
        .expect("to fit into f32");

    let affected_rows = diesel::insert_into(trade_params::table)
        .values(&(
            trade_params::protocol_id.eq(protocol_id),
            trade_params::quantity.eq(params.quantity),
            trade_params::leverage.eq(params.leverage),
            trade_params::trader_pubkey.eq(params.pubkey.to_string()),
            trade_params::direction.eq(Direction::from(params.direction)),
            trade_params::average_price.eq(average_price),
        ))
        .execute(conn)?;

    if affected_rows == 0 {
        return Err(diesel::result::Error::NotFound);
    }

    Ok(())
}

pub(crate) fn get(
    conn: &mut PgConnection,
    protocol_id: ReferenceId,
) -> QueryResult<dlc_protocol::TradeParams> {
    let protocol_id =
        util::parse_from_reference_id(protocol_id).map_err(|_| RollbackTransaction)?;
    let trade_params: TradeParams = trade_params::table
        .filter(trade_params::protocol_id.eq(protocol_id))
        .first(conn)?;

    Ok(dlc_protocol::TradeParams::from(trade_params))
}

pub(crate) fn delete(conn: &mut PgConnection, protocol_id: ReferenceId) -> QueryResult<usize> {
    let protocol_id =
        util::parse_from_reference_id(protocol_id).map_err(|_| RollbackTransaction)?;
    diesel::delete(trade_params::table)
        .filter(trade_params::protocol_id.eq(protocol_id))
        .execute(conn)
}

impl From<TradeParams> for dlc_protocol::TradeParams {
    fn from(value: TradeParams) -> Self {
        Self {
            protocol_id: value.protocol_id,
            trader: PublicKey::from_str(&value.trader_pubkey).expect("valid pubkey"),
            quantity: value.quantity,
            leverage: value.leverage,
            average_price: value.average_price,
            direction: trade::Direction::from(value.direction),
        }
    }
}
