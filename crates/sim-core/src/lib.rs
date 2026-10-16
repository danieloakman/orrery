//! Shared simulation host types: the [`Simulation`] trait and the socket protocol.

mod ipc;
mod protocol;
mod sim;

pub use ipc::{Client as IpcClient, IpcError};
pub use protocol::{
    ActionRecord, AdvanceBody, AdvanceParams, CallBody, CallParams, CreateParams, FailedCall,
    OkBody, Request, Response, RestoreParams, StateBody, StatusBody,
};
pub use sim::{ActionSpec, CallOutput, CallReturns, ParamSpec, Simulation};
