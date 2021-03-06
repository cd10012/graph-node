mod adapter;

pub use self::adapter::{
    BlockNumberRange, EthereumAdapter, EthereumBlockPointer, EthereumContractCall,
    EthereumContractCallError, EthereumContractState, EthereumContractStateError,
    EthereumContractStateRequest, EthereumEvent, EthereumEventSubscription,
    EthereumSubscriptionError,
};

pub use web3::types::BlockNumber;

pub use ethabi::{Contract, Event};
