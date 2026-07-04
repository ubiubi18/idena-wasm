use protobuf::Message;

use crate::errors::VmError;
use crate::memory::VmResult;
use crate::proto;

const ARGS_PROTOBUF_FORMAT: u8 = 0x1;
const ARGS_PLAIN_FORMAT: u8 = 0x0;

pub fn convert_args(args: &[u8]) -> VmResult<Vec<proto::models::proto_args::Argument>> {
    let mut result: Vec<proto::models::proto_args::Argument>;

    if args.is_empty() {
        return Err(VmError::custom("missing arguments format"));
    }

    match args[0] {
        ARGS_PROTOBUF_FORMAT => {
            result = proto::models::ProtoArgs::parse_from_bytes(&args[1..])
                .or(Err(VmError::custom("failed to parse arguments")))?
                .args;
        }
        ARGS_PLAIN_FORMAT => {
            let mut arg = proto::models::proto_args::Argument::new();
            arg.value = args[1..].to_vec();
            result = Vec::<proto::models::proto_args::Argument>::new();
            result.push(arg);
        }
        _ => return Err(VmError::custom("unknown format of args")),
    }
    Ok(result)
}
