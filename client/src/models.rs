use crate::schema::file_records;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use diesel::{AsExpression, FromSqlRow};

use diesel::sql_types::Integer;

#[repr(i32)]
#[derive(Debug, Clone, Copy, AsExpression, FromSqlRow, PartialEq, Deserialize, Serialize)]
#[diesel(sql_type = Integer)]
pub enum FileFormat {
    Binary = 1,
    Text = 2,
}

#[derive(Debug)]
pub enum IndexerUpdateEvent {
    Updated,
}

#[derive(Queryable, Selectable, Identifiable, Debug, Clone)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecord {
    pub id: i32,
    pub jid: Option<i32>,
    pub deleted: bool,
    pub path: String,
    pub size: i64,
    pub modified_at: OffsetDateTime,
    pub namespace_id: i32,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct CreateForm {
    pub jid: Option<i32>,
    pub path: String,
    pub deleted: bool,
    pub size: i64,
    pub modified_at: OffsetDateTime,
    pub namespace_id: i32,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct DeleteForm {
    pub path: String,
    pub jid: Option<i32>,
    pub size: i64,
    pub modified_at: OffsetDateTime,
    pub deleted: bool,
    pub namespace_id: i32,
}

#[derive(AsChangeset, Debug, Clone)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecordUpdateForm {
    pub size: i64,
    pub modified_at: OffsetDateTime,
}

#[derive(AsChangeset, Debug, Clone)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecordNonDeletedFilterForm {
    pub deleted: bool,
}

#[derive(AsChangeset, Debug, Clone)]
#[diesel(table_name = file_records)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileRecordDeleteForm {
    pub id: i32,
    pub deleted: bool,
    pub namespace_id: i32,
}

impl PartialEq<CreateForm> for FileRecord {
    fn eq(&self, other: &CreateForm) -> bool {
        self.path == other.path && self.size == other.size && self.modified_at == other.modified_at
    }
}

/// Sync status enum for communicating sync state to external callers
#[derive(Debug, Clone, uniffi::Enum)]
pub enum SyncStatus {
    /// No sync activity is currently happening
    Idle,
    /// General syncing state (used when starting sync)
    Syncing,
    /// Indexing local files to detect changes
    Indexing,
    /// Downloading files from remote server
    Downloading,
    /// Uploading files to remote server
    Uploading,
    /// An error occurred during sync
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    #[test]
    fn test_file_format_values() {
        assert_eq!(FileFormat::Binary as i32, 1);
        assert_eq!(FileFormat::Text as i32, 2);
    }

    #[test]
    fn test_file_format_equality() {
        assert_eq!(FileFormat::Binary, FileFormat::Binary);
        assert_eq!(FileFormat::Text, FileFormat::Text);
        assert_ne!(FileFormat::Binary, FileFormat::Text);
    }

    #[test]
    fn test_file_record_create_form_equality() {
        let now = OffsetDateTime::now_utc();

        let record = FileRecord {
            id: 1,
            jid: Some(100),
            deleted: false,
            path: "test/path.txt".to_string(),
            size: 1024,
            modified_at: now,
            namespace_id: 1,
        };

        let form = CreateForm {
            jid: Some(100),
            path: "test/path.txt".to_string(),
            deleted: false,
            size: 1024,
            modified_at: now,
            namespace_id: 1,
        };

        // Should be equal based on path, size, and modified_at
        assert_eq!(record, form);
    }

    #[test]
    fn test_file_record_create_form_inequality_different_path() {
        let now = OffsetDateTime::now_utc();

        let record = FileRecord {
            id: 1,
            jid: Some(100),
            deleted: false,
            path: "test/path1.txt".to_string(),
            size: 1024,
            modified_at: now,
            namespace_id: 1,
        };

        let form = CreateForm {
            jid: Some(100),
            path: "test/path2.txt".to_string(),
            deleted: false,
            size: 1024,
            modified_at: now,
            namespace_id: 1,
        };

        // Should not be equal due to different paths
        assert_ne!(record, form);
    }

    #[test]
    fn test_file_record_create_form_inequality_different_size() {
        let now = OffsetDateTime::now_utc();

        let record = FileRecord {
            id: 1,
            jid: Some(100),
            deleted: false,
            path: "test/path.txt".to_string(),
            size: 1024,
            modified_at: now,
            namespace_id: 1,
        };

        let form = CreateForm {
            jid: Some(100),
            path: "test/path.txt".to_string(),
            deleted: false,
            size: 2048,
            modified_at: now,
            namespace_id: 1,
        };

        // Should not be equal due to different sizes
        assert_ne!(record, form);
    }

    #[test]
    fn test_create_form_construction() {
        let now = OffsetDateTime::now_utc();

        let form = CreateForm {
            jid: Some(42),
            path: "recipes/test.cook".to_string(),
            deleted: false,
            size: 512,
            modified_at: now,
            namespace_id: 5,
        };

        assert_eq!(form.jid, Some(42));
        assert_eq!(form.path, "recipes/test.cook");
        assert_eq!(form.deleted, false);
        assert_eq!(form.size, 512);
        assert_eq!(form.namespace_id, 5);
    }

    #[test]
    fn test_delete_form_construction() {
        let now = OffsetDateTime::now_utc();

        let form = DeleteForm {
            path: "recipes/deleted.cook".to_string(),
            jid: Some(99),
            size: 0,
            modified_at: now,
            deleted: true,
            namespace_id: 1,
        };

        assert_eq!(form.path, "recipes/deleted.cook");
        assert_eq!(form.deleted, true);
        assert_eq!(form.size, 0);
    }

    #[test]
    fn test_file_record_update_form() {
        let now = OffsetDateTime::now_utc();

        let update_form = FileRecordUpdateForm {
            size: 2048,
            modified_at: now,
        };

        assert_eq!(update_form.size, 2048);
        assert_eq!(update_form.modified_at, now);
    }

    #[test]
    fn test_sync_status_variants() {
        let idle = SyncStatus::Idle;
        let syncing = SyncStatus::Syncing;
        let indexing = SyncStatus::Indexing;
        let downloading = SyncStatus::Downloading;
        let uploading = SyncStatus::Uploading;
        let error = SyncStatus::Error {
            message: "Test error".to_string(),
        };

        // Just verify they can be constructed
        assert!(matches!(idle, SyncStatus::Idle));
        assert!(matches!(syncing, SyncStatus::Syncing));
        assert!(matches!(indexing, SyncStatus::Indexing));
        assert!(matches!(downloading, SyncStatus::Downloading));
        assert!(matches!(uploading, SyncStatus::Uploading));
        assert!(matches!(error, SyncStatus::Error { .. }));
    }
}
