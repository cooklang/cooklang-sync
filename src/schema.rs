// @generated automatically by Diesel CLI.

diesel::table! {
    file_records (id) {
        id -> Integer,
        jid -> Integer,
        path -> Text,
        format -> Text,
        modified_at -> Nullable<Text>,
        size -> Nullable<Integer>,
        created_at -> Text,
    }
}
