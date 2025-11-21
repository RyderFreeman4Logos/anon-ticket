use sea_orm::sea_query::{ColumnDef, Expr, Table, TableCreateStatement};
use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection};

use crate::entity::{monitor_state, payments, service_tokens};
use anon_ticket_domain::storage::StorageResult;

pub async fn run_migrations(db: &DatabaseConnection) -> StorageResult<()> {
    let backend = db.get_database_backend();

    let payments_table = Table::create()
        .if_not_exists()
        .table(payments::Entity)
        .col(
            ColumnDef::new(payments::Column::Pid)
                .binary_len(32)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(payments::Column::Txid)
                .string_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(payments::Column::Amount)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(payments::Column::BlockHeight)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(payments::Column::Status)
                .string_len(16)
                .not_null(),
        )
        .col(
            ColumnDef::new(payments::Column::CreatedAt)
                .date_time()
                .not_null()
                .default(Expr::current_timestamp()),
        )
        .col(
            ColumnDef::new(payments::Column::ClaimedAt)
                .date_time()
                .null(),
        )
        .to_owned();
    create_table(db, backend, payments_table).await?;

    let service_tokens_table = Table::create()
        .if_not_exists()
        .table(service_tokens::Entity)
        .col(
            ColumnDef::new(service_tokens::Column::Token)
                .binary_len(32)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(service_tokens::Column::Pid)
                .binary_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(service_tokens::Column::Amount)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(service_tokens::Column::IssuedAt)
                .date_time()
                .not_null()
                .default(Expr::current_timestamp()),
        )
        .col(
            ColumnDef::new(service_tokens::Column::RevokedAt)
                .date_time()
                .null(),
        )
        .col(
            ColumnDef::new(service_tokens::Column::RevokeReason)
                .string()
                .null(),
        )
        .col(
            ColumnDef::new(service_tokens::Column::AbuseScore)
                .small_integer()
                .not_null()
                .default(0),
        )
        .to_owned();
    create_table(db, backend, service_tokens_table).await?;

    let monitor_table = Table::create()
        .if_not_exists()
        .table(monitor_state::Entity)
        .col(
            ColumnDef::new(monitor_state::Column::Key)
                .string_len(64)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(monitor_state::Column::ValueInt)
                .big_integer()
                .not_null(),
        )
        .to_owned();
    create_table(db, backend, monitor_table).await?;

    Ok(())
}

async fn create_table(
    db: &DatabaseConnection,
    backend: DatabaseBackend,
    mut statement: TableCreateStatement,
) -> StorageResult<()> {
    statement.if_not_exists();
    db.execute(backend.build(&statement))
        .await
        .map_err(crate::errors::StorageError::from_source)?;
    Ok(())
}
