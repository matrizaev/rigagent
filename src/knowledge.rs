use rig::Embed;
use rig_sqlite::{Column, ColumnValue, SqliteVectorStoreTable};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Embed, Eq, PartialEq, Serialize)]
pub struct SupportDoc {
    pub id: String,
    pub title: String,
    pub source: String,
    pub category: String,
    #[embed]
    pub content: String,
}

impl SqliteVectorStoreTable for SupportDoc {
    fn name() -> &'static str {
        "support_docs"
    }

    fn schema() -> Vec<Column> {
        vec![
            Column::new("id", "TEXT PRIMARY KEY"),
            Column::new("title", "TEXT"),
            Column::new("source", "TEXT"),
            Column::new("category", "TEXT").indexed(),
            Column::new("content", "TEXT"),
        ]
    }

    fn id(&self) -> String {
        self.id.clone()
    }

    fn column_values(&self) -> Vec<(&'static str, Box<dyn ColumnValue>)> {
        vec![
            ("id", Box::new(self.id.clone())),
            ("title", Box::new(self.title.clone())),
            ("source", Box::new(self.source.clone())),
            ("category", Box::new(self.category.clone())),
            ("content", Box::new(self.content.clone())),
        ]
    }
}
