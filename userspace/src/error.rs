use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum VtlError {
    #[error("数据库错误: {0}")]
    DatabaseError(#[from] rusqlite::Error),
    #[error("IO 错误: {0}")]
    IoError(#[from] std::io::Error),
    #[error("容量/大小格式无效: {0}")]
    InvalidSize(String),
    #[error("未找到磁带: {0}")]
    TapeNotFound(String),
    #[error("槽位已被占用")]
    #[allow(dead_code)]
    SlotOccupied,
    #[error("槽位为空")]
    SlotEmpty,
    #[error("驱动器为空")]
    DriveEmpty,
    #[error("驱动器忙")]
    DriveBusy,
    #[error("参数无效: {0}")]
    InvalidParameter(String),
    #[error("权限被拒绝: {0}")]
    PermissionDenied(String),
    #[error("无可用槽位")]
    NoAvailableSlots,
    #[error("磁带名称无效: {0}")]
    InvalidTapeName(String),
    #[error("超出配额: {0}")]
    QuotaExceeded(String),
    #[error("未找到标签: {0}")]
    TagNotFound(String),
    #[error("未找到磁带库: {0}")]
    LibraryNotFound(String),
    #[error("未找到磁带架: {0}")]
    ShelfNotFound(String),
    #[error("磁带库已存在: {0}")]
    LibraryExists(String),
    #[error("磁带当前在驱动器中，请先卸载后再操作")]
    TapeInDrive,
    #[error("磁带须先位于货架上（不得仅在机械手槽内或驱动中）；请先回架后再试")]
    TapeNotOnShelf,
}
