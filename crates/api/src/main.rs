// 声明模块结构：
// `application`: 包含应用启动逻辑。
// `handlers`: 包含具体的 API 请求处理逻辑。
// `state`: 包含应用共享状态定义。
mod application;
mod handlers;
mod state;

// 仅在测试配置下编译 `tests` 模块。
#[cfg(test)]
mod tests;

use std::io;

// `#[actix_web::main]` 宏将异步 main 函数标记为 actix-web 程序的入口点。
// 它会在后台启动 actix 的系统运行时。
#[actix_web::main]
async fn main() -> io::Result<()> {
    // 调用 application 模块的 run 函数启动服务器。
    // 如果启动失败，捕获错误并打印到标准错误输出，然后返回 IO 错误以非零状态码退出。
    if let Err(err) = application::run().await {
        eprintln!("[api] bootstrap failed: {err}");
        return Err(io::Error::other(err.to_string()));
    }

    Ok(())
}