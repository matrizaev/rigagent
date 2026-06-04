use rig::tool::ToolError;
use rig::tool_macro;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrderStatus {
    pub order_id: String,
    pub found: bool,
    pub status: Option<String>,
    pub estimated_delivery: Option<String>,
    pub carrier: Option<String>,
    pub tracking_number: Option<String>,
    pub note: String,
}

#[tool_macro(description = "List existing order IDs.")]
pub fn list_orders() -> Result<Vec<String>, ToolError> {
    Ok(vec![
        "RIG-1001".to_string(),
        "RIG-1002".to_string(),
        "RIG-1003".to_string(),
    ])
}

#[tool_macro(
    description = "Look up a demo order and return its current fulfillment status.",
    params(order_id = "Demo order id such as RIG-1001, RIG-1002, or RIG-1003.")
)]
pub fn lookup_order_status(order_id: String) -> Result<OrderStatus, ToolError> {
    let order_id = order_id.trim().to_ascii_uppercase();
    let status = match order_id.as_str() {
        "RIG-1001" => OrderStatus {
            order_id,
            found: true,
            status: Some("shipped".to_string()),
            estimated_delivery: Some("2026-06-06".to_string()),
            carrier: Some("UPS".to_string()),
            tracking_number: Some("1Z999RIG1001".to_string()),
            note: "Shipment left the regional facility and is in transit.".to_string(),
        },
        "RIG-1002" => OrderStatus {
            order_id,
            found: true,
            status: Some("processing".to_string()),
            estimated_delivery: Some("2026-06-10".to_string()),
            carrier: None,
            tracking_number: None,
            note: "Order is being packed. Tracking appears after carrier handoff.".to_string(),
        },
        "RIG-1003" => OrderStatus {
            order_id,
            found: true,
            status: Some("delivered".to_string()),
            estimated_delivery: Some("2026-05-28".to_string()),
            carrier: Some("FedEx".to_string()),
            tracking_number: Some("612999RIG1003".to_string()),
            note: "Delivered to front desk.".to_string(),
        },
        _ => OrderStatus {
            order_id,
            found: false,
            status: None,
            estimated_delivery: None,
            carrier: None,
            tracking_number: None,
            note: "No demo order matched that id. Try RIG-1001, RIG-1002, or RIG-1003.".to_string(),
        },
    };

    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_order_status_returns_known_order() {
        let status =
            lookup_order_status("rig-1001".to_string()).expect("known order should resolve");

        assert!(status.found);
        assert_eq!(status.status.as_deref(), Some("shipped"));
        assert_eq!(status.carrier.as_deref(), Some("UPS"));
    }

    #[test]
    fn lookup_order_status_returns_missing_order() {
        let status =
            lookup_order_status("missing".to_string()).expect("missing order is a valid lookup");

        assert!(!status.found);
        assert!(status.status.is_none());
    }
}
