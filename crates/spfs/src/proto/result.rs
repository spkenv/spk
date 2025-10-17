// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

// This trait causes dead code warnings when the server feature is not enabled.
#[allow(dead_code)]
pub(crate) trait RpcResult: Sized {
    type Ok;
    type Result;
    fn error(err: crate::Error) -> Self;
    fn ok(data: Self::Ok) -> Self;
    fn to_result(self) -> crate::Result<Self::Ok>;
    fn from_result<T: Into<Self::Ok>>(result: crate::Result<T>) -> Self {
        match result {
            Err(err) => Self::error(err),
            Ok(ok) => Self::ok(ok.into()),
        }
    }
}

#[cfg(feature = "server")]
macro_rules! handle_error {
    ($result:expr) => {
        match $result {
            Err(err) => return Ok(tonic::Response::new(RpcResult::error(err))),
            Ok(data) => data,
        }
    };
}

#[cfg(feature = "server")]
pub(crate) use handle_error;

macro_rules! rpc_result {
    ($typename:ty, $result:ty) => {
        rpc_result!($typename, $result, super::Ok);
    };
    ($typename:ty, $result:ty, $ok_type:ty) => {
        impl RpcResult for $typename {
            type Ok = $ok_type;
            type Result = $result;
            fn error(err: crate::Error) -> Self {
                let result = Some(Self::Result::Error(err.into()));
                Self { result }
            }
            fn ok(data: Self::Ok) -> Self {
                let result = Some(Self::Result::Ok(data));
                Self { result }
            }
            fn to_result(self) -> crate::Result<Self::Ok> {
                match self.result {
                    Some(Self::Result::Error(err)) => Err(err.into()),
                    Some(Self::Result::Ok(data)) => Ok(data),
                    None => Err(crate::Error::String(format!(
                        "Unexpected empty result from the server"
                    ))),
                }
            }
        }
    };
}

use super::generated as g;

rpc_result!(
    g::LsTagsResponse,
    g::ls_tags_response::Result,
    g::ls_tags_response::EntryList
);
rpc_result!(
    g::ResolveTagResponse,
    g::resolve_tag_response::Result,
    g::Tag
);
rpc_result!(
    g::FindTagsResponse,
    g::find_tags_response::Result,
    g::find_tags_response::TagList
);
rpc_result!(
    g::IterTagSpecsResponse,
    g::iter_tag_specs_response::Result,
    g::iter_tag_specs_response::TagSpecList
);
rpc_result!(
    g::ReadTagResponse,
    g::read_tag_response::Result,
    g::read_tag_response::TagList
);
rpc_result!(g::InsertTagResponse, g::insert_tag_response::Result);
rpc_result!(
    g::RemoveTagStreamResponse,
    g::remove_tag_stream_response::Result
);
rpc_result!(g::RemoveTagResponse, g::remove_tag_response::Result);

rpc_result!(
    g::ReadObjectResponse,
    g::read_object_response::Result,
    g::Object
);
rpc_result!(
    g::FindDigestsResponse,
    g::find_digests_response::Result,
    g::FoundDigest
);
rpc_result!(
    g::IterDigestsResponse,
    g::iter_digests_response::Result,
    g::Digest
);
rpc_result!(
    g::IterObjectsResponse,
    g::iter_objects_response::Result,
    g::Object
);
rpc_result!(
    g::WalkObjectsResponse,
    g::walk_objects_response::Result,
    g::walk_objects_response::WalkObjectsItem
);
rpc_result!(g::WriteObjectResponse, g::write_object_response::Result);
rpc_result!(g::RemoveObjectResponse, g::remove_object_response::Result);
rpc_result!(
    g::RemoveObjectIfOlderThanResponse,
    g::remove_object_if_older_than_response::Result,
    bool
);

rpc_result!(
    g::PayloadSizeResponse,
    g::payload_size_response::Result,
    u64
);
rpc_result!(
    g::WritePayloadResponse,
    g::write_payload_response::Result,
    g::write_payload_response::UploadOption
);
rpc_result!(
    g::OpenPayloadResponse,
    g::open_payload_response::Result,
    g::open_payload_response::DownloadOption
);
rpc_result!(g::RemovePayloadResponse, g::remove_payload_response::Result);
rpc_result!(
    g::write_payload_response::UploadResponse,
    g::write_payload_response::upload_response::Result,
    g::write_payload_response::upload_response::UploadResult
);
rpc_result!(
    g::RemovePayloadIfOlderThanResponse,
    g::remove_payload_if_older_than_response::Result,
    bool
);
