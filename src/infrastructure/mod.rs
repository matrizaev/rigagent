//! External adapters for persistence, provider calls, IDs, clocks, and files.

/// Decision-agent adapters.
pub mod agents;

/// Diesel-backed persistence adapters.
pub mod persistence;

/// Scenario YAML loading for seed data.
pub mod scenario;
