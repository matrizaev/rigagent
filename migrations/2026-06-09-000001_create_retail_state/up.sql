CREATE TABLE shop_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    current_date TEXT NOT NULL,
    capacity_space_units INTEGER NOT NULL CHECK (capacity_space_units >= 0)
);

CREATE TABLE products (
    sku TEXT PRIMARY KEY,
    item_type TEXT NOT NULL,
    brand TEXT NOT NULL,
    size TEXT NOT NULL,
    unit_cost_cents INTEGER NOT NULL CHECK (unit_cost_cents >= 0),
    unit_price_cents INTEGER NOT NULL CHECK (unit_price_cents >= 0),
    space_units INTEGER NOT NULL CHECK (space_units >= 0),
    daily_demand_milli_units INTEGER NOT NULL CHECK (daily_demand_milli_units >= 0),
    restock_lead_time_days INTEGER NOT NULL CHECK (restock_lead_time_days > 0),
    min_order_quantity INTEGER NOT NULL CHECK (min_order_quantity > 0),
    max_order_quantity INTEGER NOT NULL CHECK (max_order_quantity >= min_order_quantity),
    active BOOLEAN NOT NULL
);

CREATE TABLE inventory (
    sku TEXT PRIMARY KEY REFERENCES products(sku),
    on_hand INTEGER NOT NULL CHECK (on_hand >= 0),
    demand_backlog_milli_units INTEGER NOT NULL CHECK (
        demand_backlog_milli_units >= 0
        AND demand_backlog_milli_units < 1000
    )
);

CREATE TABLE decision_runs (
    id TEXT PRIMARY KEY,
    decision_date TEXT NOT NULL,
    horizon_days INTEGER NOT NULL CHECK (horizon_days > 0),
    status TEXT NOT NULL CHECK (status IN ('started', 'completed', 'failed')),
    summary TEXT,
    created_restock_count INTEGER NOT NULL CHECK (created_restock_count >= 0)
);

CREATE TABLE sales_orders (
    id TEXT PRIMARY KEY,
    sale_date TEXT NOT NULL,
    sku TEXT NOT NULL REFERENCES products(sku),
    quantity_requested INTEGER NOT NULL CHECK (quantity_requested >= 0),
    quantity_fulfilled INTEGER NOT NULL CHECK (quantity_fulfilled >= 0),
    revenue_cents INTEGER NOT NULL CHECK (revenue_cents >= 0),
    cost_cents INTEGER NOT NULL CHECK (cost_cents >= 0),
    lost_units INTEGER NOT NULL CHECK (lost_units >= 0),
    CHECK (quantity_fulfilled <= quantity_requested)
);

CREATE TABLE restock_orders (
    id TEXT PRIMARY KEY,
    sku TEXT NOT NULL REFERENCES products(sku),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    ordered_at TEXT NOT NULL,
    eta_date TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('open', 'received', 'cancelled')),
    decision_run_id TEXT NOT NULL REFERENCES decision_runs(id),
    rationale TEXT NOT NULL
);

CREATE INDEX restock_orders_open_sku_eta_idx
    ON restock_orders (sku, eta_date)
    WHERE status = 'open';
