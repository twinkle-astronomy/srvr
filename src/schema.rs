// @generated automatically by Diesel CLI.

diesel::table! {
    devices (id) {
        id -> Integer,
        mac_address -> Text,
        friendly_name -> Nullable<Text>,
        api_key -> Text,
        firmware_version -> Nullable<Text>,
        created_at -> Timestamp,
        last_seen_at -> Nullable<Timestamp>,
    }
}
