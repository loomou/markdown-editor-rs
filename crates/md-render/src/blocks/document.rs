use super::BlockComponent;

pub(crate) struct DocRootBlock;
pub(crate) struct DocStartBlock;

impl BlockComponent for DocRootBlock {}

impl BlockComponent for DocStartBlock {}
