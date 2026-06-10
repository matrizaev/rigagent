//! Private Diesel schema for retail persistence.

diesel::table! {
    shop_state (id) {
        id -> Integer,
        current_date -> Text,
        capacity_space_units -> BigInt,
    }
}

diesel::table! {
    products (sku) {
        sku -> Text,
        item_type -> Text,
        brand -> Text,
        size -> Text,
        unit_cost_cents -> BigInt,
        unit_price_cents -> BigInt,
        space_units -> BigInt,
        daily_demand_milli_units -> BigInt,
        restock_lead_time_days -> BigInt,
        min_order_quantity -> BigInt,
        max_order_quantity -> BigInt,
        active -> Bool,
    }
}

diesel::table! {
    inventory (sku) {
        sku -> Text,
        on_hand -> BigInt,
        demand_backlog_milli_units -> BigInt,
    }
}

diesel::table! {
    decision_runs (id) {
        id -> Text,
        decision_date -> Text,
        horizon_days -> BigInt,
        status -> Text,
        summary -> Nullable<Text>,
        created_restock_count -> BigInt,
    }
}

diesel::table! {
    sales_orders (id) {
        id -> Text,
        sale_date -> Text,
        sku -> Text,
        quantity_requested -> BigInt,
        quantity_fulfilled -> BigInt,
        revenue_cents -> BigInt,
        cost_cents -> BigInt,
        lost_units -> BigInt,
    }
}

diesel::table! {
    restock_orders (id) {
        id -> Text,
        sku -> Text,
        quantity -> BigInt,
        ordered_at -> Text,
        eta_date -> Text,
        status -> Text,
        decision_run_id -> Text,
        rationale -> Text,
    }
}

diesel::joinable!(inventory -> products (sku));
diesel::joinable!(restock_orders -> decision_runs (decision_run_id));
diesel::joinable!(restock_orders -> products (sku));
diesel::joinable!(sales_orders -> products (sku));

diesel::allow_tables_to_appear_in_same_query!(
    shop_state,
    products,
    inventory,
    decision_runs,
    sales_orders,
    restock_orders,
);
