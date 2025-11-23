pub mod payments {
    use sea_orm::entity::prelude::*;
    use sea_orm::sea_query::Expr;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "payments")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub pid: Vec<u8>,
        pub txid: String,
        pub amount: i64,
        pub block_height: i64,
        pub status: PaymentStatusDb,
        #[sea_orm(default_expr = "Expr::current_timestamp()")]
        pub created_at: DateTimeUtc,
        pub claimed_at: Option<DateTimeUtc>,
    }

    #[derive(Copy, Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
    #[sea_orm(rs_type = "i8", db_type = "TinyInteger")]
    pub enum PaymentStatusDb {
        #[sea_orm(num_value = 0)]
        Unclaimed,
        #[sea_orm(num_value = 1)]
        Claimed,
    }

    #[derive(Debug, Clone, Copy, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod service_tokens {
    use sea_orm::entity::prelude::*;
    use sea_orm::sea_query::Expr;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "service_tokens")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub token: Vec<u8>,
        pub pid: Vec<u8>,
        pub amount: i64,
        #[sea_orm(default_expr = "Expr::current_timestamp()")]
        pub issued_at: DateTimeUtc,
        pub revoked_at: Option<DateTimeUtc>,
        pub revoke_reason: Option<String>,
        #[sea_orm(default_value = 0)]
        pub abuse_score: i16,
    }

    #[derive(Debug, Clone, Copy, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod monitor_state {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "monitor_state")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub key: String,
        pub value_int: i64,
    }

    #[derive(Debug, Clone, Copy, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
