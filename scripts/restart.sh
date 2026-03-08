#!/bin/bash
# OpenFang 后台重启脚本
# 用法: ./scripts/restart.sh [dev|release] [log_level]
# 默认: release 模式, info 日志级别
# 日志级别: trace, debug, info, warn, error

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 项目根目录
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

# 构建模式: dev 或 release
BUILD_MODE="${1:-release}"

# 日志级别: trace, debug, info, warn, error
LOG_LEVEL="${2:-info}"

# 验证日志级别
VALID_LEVELS="trace debug info warn error"
if [[ ! " $VALID_LEVELS " =~ " $LOG_LEVEL " ]]; then
    echo -e "${RED}错误: 无效的日志级别 '$LOG_LEVEL'${NC}"
    echo "有效级别: trace, debug, info, warn, error"
    exit 1
fi

if [[ "$BUILD_MODE" == "dev" || "$BUILD_MODE" == "debug" ]]; then
    BUILD_FLAG=""
    BINARY_PATH="target/debug/openfang"
    MODE_NAME="debug"
else
    BUILD_FLAG="--release"
    BINARY_PATH="target/release/openfang"
    MODE_NAME="release"
fi

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  OpenFang 后台重启脚本 (${MODE_NAME}模式)${NC}"
echo -e "${BLUE}  日志级别: ${LOG_LEVEL}${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# 1. 查找并停止正在运行的 openfang 进程
echo -e "${YELLOW}[1/5] 检查并停止现有进程...${NC}"

# 尝试使用 openfang CLI 优雅地停止
if command -v "$BINARY_PATH" &>/dev/null; then
    echo "尝试通过 CLI 停止守护进程..."
    "$BINARY_PATH" stop 2>/dev/null || true
    sleep 2
fi

# 强制终止任何残留的进程
PIDS=$(pgrep -f "openfang" || true)
if [ -n "$PIDS" ]; then
    echo -e "${YELLOW}发现残留进程: $PIDS，正在终止...${NC}"
    echo "$PIDS" | xargs kill -9 2>/dev/null || true
    sleep 2
else
    echo "没有发现运行中的进程"
fi

# 等待端口释放
for port in 4200 50051; do
    if lsof -Pi :$port -sTCP:LISTEN -t >/dev/null 2>&1; then
        echo "等待端口 $port 释放..."
        sleep 3
    fi
done

echo -e "${GREEN}✓ 进程已清理${NC}"
echo ""

# 2. 构建项目
echo -e "${YELLOW}[2/5] 构建项目 (${MODE_NAME}模式)...${NC}"
echo "执行: cargo build $BUILD_FLAG -p openfang-cli"
echo "注意: 首次构建可能需要 10-30 分钟，请耐心等待..."
echo ""

# 设置较长的超时时间（600秒 = 10分钟）
export CARGO_NET_TIMEOUT=600
export CARGO_HTTP_TIMEOUT=600

# 使用 timeout 命令确保构建有足够的时间（900秒 = 15分钟）
if command -v timeout >/dev/null 2>&1; then
    timeout 900 cargo build $BUILD_FLAG -p openfang-cli 2>&1 | tee /tmp/openfang_build.log
else
    # 如果没有 timeout 命令，直接运行
    cargo build $BUILD_FLAG -p openfang-cli 2>&1 | tee /tmp/openfang_build.log
fi

BUILD_EXIT_CODE=${PIPESTATUS[0]}

if [ $BUILD_EXIT_CODE -eq 124 ]; then
    echo -e "${RED}✗ 构建超时! 构建时间超过 15 分钟${NC}"
    echo "建议: 使用 release 模式构建，或检查网络连接"
    exit 1
elif [ $BUILD_EXIT_CODE -ne 0 ]; then
    echo -e "${RED}✗ 构建失败! 查看错误日志: /tmp/openfang_build.log${NC}"
    exit 1
fi

if [ ! -f "$BINARY_PATH" ]; then
    echo -e "${RED}✗ 构建成功但未找到二进制文件: $BINARY_PATH${NC}"
    exit 1
fi

echo -e "${GREEN}✓ 构建成功: $BINARY_PATH${NC}"
echo ""

# 3. 检查配置文件
echo -e "${YELLOW}[3/5] 检查配置文件...${NC}"

CONFIG_DIR="$HOME/.openfang"
CONFIG_FILE="$CONFIG_DIR/config.toml"

if [ ! -f "$CONFIG_FILE" ]; then
    echo -e "${YELLOW}配置文件不存在，创建默认配置...${NC}"
    mkdir -p "$CONFIG_DIR"
    cat > "$CONFIG_FILE" << 'EOF'
# OpenFang Agent OS 配置
api_listen = "127.0.0.1:4200"

[default_model]
provider = "moonshot"
model = "kimi-latest"
api_key_env = "MOONSHOT_API_KEY"

[memory]
decay_rate = 0.05
EOF
    echo -e "${GREEN}✓ 已创建默认配置: $CONFIG_FILE${NC}"
else
    echo -e "${GREEN}✓ 配置文件存在: $CONFIG_FILE${NC}"
fi

# 检查并同步环境变量
# OpenFang 只加载 ~/.openfang/.env，所以将项目 .env 同步过去
if [ -f "$PROJECT_ROOT/.env" ]; then
    echo "发现项目 .env 文件，同步到 ~/.openfang/.env"
    mkdir -p "$CONFIG_DIR"
    cp "$PROJECT_ROOT/.env" "$CONFIG_DIR/.env"
    chmod 600 "$CONFIG_DIR/.env"
    echo -e "${GREEN}✓ 已同步环境变量${NC}"
fi

# 将日志级别写入 .env 文件（RUST_LOG 环境变量）- 放在同步之后，确保生效
if grep -q "^RUST_LOG=" "$CONFIG_DIR/.env" 2>/dev/null; then
    sed -i.bak "s/^RUST_LOG=.*/RUST_LOG=$LOG_LEVEL/" "$CONFIG_DIR/.env" && rm -f "$CONFIG_DIR/.env.bak"
else
    echo "RUST_LOG=$LOG_LEVEL" >> "$CONFIG_DIR/.env"
fi
echo -e "${GREEN}✓ 已设置日志级别: RUST_LOG=$LOG_LEVEL${NC}"

# 加载 ~/.openfang/.env 到当前环境（用于构建和检查）
if [ -f "$CONFIG_DIR/.env" ]; then
    set -a
    source "$CONFIG_DIR/.env"
    set +a
fi

# 检查 API key
if [ -z "$MOONSHOT_API_KEY" ] && [ -z "$ANTHROPIC_API_KEY" ] && [ -z "$OPENAI_API_KEY" ] && [ -z "$GROQ_API_KEY" ]; then
    echo -e "${YELLOW}⚠ 警告: 未检测到任何 LLM API Key 环境变量${NC}"
    echo "请确保设置以下环境变量之一:"
    echo "  - MOONSHOT_API_KEY (Kimi)"
    echo "  - ANTHROPIC_API_KEY"
    echo "  - OPENAI_API_KEY"
    echo "  - GROQ_API_KEY"
fi

# 检查飞书配置
if [ -z "$FEISHU_APP_SECRET" ]; then
    # 尝试从配置文件读取 app_secret
    FEISHU_SECRET=$(grep -A 10 "^\[channels.feishu\]" "$CONFIG_FILE" 2>/dev/null | grep "^app_secret = " | head -1 | sed 's/.*app_secret = "\(.*\)".*/\1/')
    if [ -n "$FEISHU_SECRET" ] && [ "$FEISHU_SECRET" != "rB2T6UN8puFRJdRx0bW9fgyPTwcLdSMO" ]; then
        echo "FEISHU_APP_SECRET=$FEISHU_SECRET" >> "$CONFIG_DIR/.env"
        export FEISHU_APP_SECRET="$FEISHU_SECRET"
        echo -e "${GREEN}✓ 已从配置文件提取 FEISHU_APP_SECRET${NC}"
    else
        echo -e "${YELLOW}⚠ 警告: FEISHU_APP_SECRET 未设置，飞书功能可能无法正常工作${NC}"
    fi
fi

echo ""

# 4. 启动守护进程
echo -e "${YELLOW}[4/5] 启动 OpenFang 守护进程...${NC}"

# 使用 nohup 在后台启动
LOG_FILE="$CONFIG_DIR/daemon.log"

# 设置 RUST_LOG 环境变量（OpenFang 使用它来控制日志级别）
export RUST_LOG="$LOG_LEVEL"

echo "日志级别: RUST_LOG=$RUST_LOG"
nohup "$BINARY_PATH" start > "$LOG_FILE" 2>&1 &
DAEMON_PID=$!

echo "守护进程 PID: $DAEMON_PID"
echo "日志文件: $LOG_FILE"

# 等待进程启动
sleep 5

# 检查进程是否还在运行
if ! kill -0 $DAEMON_PID 2>/dev/null; then
    echo -e "${RED}✗ 守护进程启动失败，查看日志:${NC}"
    tail -50 "$LOG_FILE"
    exit 1
fi

echo -e "${GREEN}✓ 守护进程已启动${NC}"
echo ""

# 5. 健康检查
echo -e "${YELLOW}[5/5] 健康检查...${NC}"

HEALTH_URL="http://127.0.0.1:4200/api/health"
MAX_RETRIES=10
RETRY_COUNT=0

while [ $RETRY_COUNT -lt $MAX_RETRIES ]; do
    if curl -s "$HEALTH_URL" > /dev/null 2>&1; then
        echo -e "${GREEN}✓ 服务健康检查通过${NC}"
        echo ""
        echo -e "${GREEN}========================================${NC}"
        echo -e "${GREEN}  OpenFang 重启成功!${NC}"
        echo -e "${GREEN}========================================${NC}"
        echo ""
        echo "服务信息:"
        echo "  - API 地址: http://127.0.0.1:4200"
        echo "  - 健康检查: $HEALTH_URL"
        echo "  - 日志文件: $LOG_FILE"
        echo "  - 配置文件: $CONFIG_FILE"
        echo ""
        echo "常用命令:"
        echo "  - 查看状态: $BINARY_PATH status"
        echo "  - 停止服务: $BINARY_PATH stop"
        echo "  - 查看日志: tail -f $LOG_FILE"
        echo "  - 日志级别: $LOG_LEVEL (trace/debug/info/warn/error)"
        echo ""
        
        # 显示状态
        sleep 1
        curl -s "$HEALTH_URL" | head -20 || true
        
        exit 0
    fi
    
    RETRY_COUNT=$((RETRY_COUNT + 1))
    echo "  等待服务就绪... ($RETRY_COUNT/$MAX_RETRIES)"
    sleep 2
done

echo -e "${RED}✗ 健康检查失败，服务可能未正常启动${NC}"
echo "查看日志:"
tail -50 "$LOG_FILE"
exit 1
