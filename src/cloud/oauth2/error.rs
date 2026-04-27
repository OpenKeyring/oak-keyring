//! OAuth2 authorization errors.

#[derive(Debug, thiserror::Error)]
pub enum OAuth2Error {
    #[error("授权超时，请重试")]
    Timeout,

    #[error("回调中未收到 authorization code")]
    MissingCode,

    #[error("Token 交换失败: {message}")]
    TokenExchange { message: String },

    #[error("端口 {port} 被占用")]
    PortInUse { port: u16 },

    #[error("无法打开浏览器: {message}")]
    BrowserOpen { message: String },

    #[error("用户取消授权")]
    Cancelled,

    #[error("Token 存储失败: {message}")]
    TokenStore { message: String },

    #[error("提供商 '{provider}' 未授权")]
    ProviderNotAuthorized { provider: String },
}
