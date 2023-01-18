// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Message {
    #[prost(message, optional, tag = "1")]
    pub wantlist: ::core::option::Option<message::Wantlist>,
    /// used to send Blocks in bitswap 1.0.0
    #[prost(bytes = "vec", repeated, tag = "2")]
    pub blocks: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
    /// used to send Blocks in bitswap 1.1.0
    #[prost(message, repeated, tag = "3")]
    pub payload: ::prost::alloc::vec::Vec<message::Block>,
    #[prost(message, repeated, tag = "4")]
    pub block_presences: ::prost::alloc::vec::Vec<message::BlockPresence>,
    #[prost(int32, tag = "5")]
    pub pending_bytes: i32,
}
/// Nested message and enum types in `Message`.
pub mod message {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Wantlist {
        /// a list of wantlist entries
        #[prost(message, repeated, tag = "1")]
        pub entries: ::prost::alloc::vec::Vec<wantlist::Entry>,
        /// whether this is the full wantlist. default to false
        #[prost(bool, tag = "2")]
        pub full: bool,
    }
    /// Nested message and enum types in `Wantlist`.
    pub mod wantlist {
        #[allow(clippy::derive_partial_eq_without_eq)]
        #[derive(Clone, PartialEq, ::prost::Message)]
        pub struct Entry {
            /// the block cid (cidV0 in bitswap 1.0.0, cidV1 in bitswap 1.1.0)
            #[prost(bytes = "vec", tag = "1")]
            pub block: ::prost::alloc::vec::Vec<u8>,
            /// the priority (normalized). default to 1
            #[prost(int32, tag = "2")]
            pub priority: i32,
            /// whether this revokes an entry
            #[prost(bool, tag = "3")]
            pub cancel: bool,
            /// Note: defaults to enum 0, ie Block
            #[prost(enumeration = "WantType", tag = "4")]
            pub want_type: i32,
            /// Note: defaults to false
            #[prost(bool, tag = "5")]
            pub send_dont_have: bool,
        }
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration,
        )]
        #[repr(i32)]
        pub enum WantType {
            Block = 0,
            Have = 1,
        }
        impl WantType {
            /// String value of the enum field names used in the ProtoBuf definition.
            ///
            /// The values are not transformed in any way and thus are considered stable
            /// (if the ProtoBuf definition does not change) and safe for programmatic use.
            pub fn as_str_name(&self) -> &'static str {
                match self {
                    WantType::Block => "Block",
                    WantType::Have => "Have",
                }
            }
            /// Creates an enum from field names used in the ProtoBuf definition.
            pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
                match value {
                    "Block" => Some(Self::Block),
                    "Have" => Some(Self::Have),
                    _ => None,
                }
            }
        }
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct Block {
        /// CID prefix (cid version, multicodec and multihash prefix (type + length)
        #[prost(bytes = "vec", tag = "1")]
        pub prefix: ::prost::alloc::vec::Vec<u8>,
        #[prost(bytes = "vec", tag = "2")]
        pub data: ::prost::alloc::vec::Vec<u8>,
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Message)]
    pub struct BlockPresence {
        #[prost(bytes = "vec", tag = "1")]
        pub cid: ::prost::alloc::vec::Vec<u8>,
        #[prost(enumeration = "BlockPresenceType", tag = "2")]
        pub r#type: i32,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum BlockPresenceType {
        Have = 0,
        DontHave = 1,
    }
    impl BlockPresenceType {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                BlockPresenceType::Have => "Have",
                BlockPresenceType::DontHave => "DontHave",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "Have" => Some(Self::Have),
                "DontHave" => Some(Self::DontHave),
                _ => None,
            }
        }
    }
}
