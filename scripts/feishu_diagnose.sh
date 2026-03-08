#!/bin/bash
# 飞书连接诊断脚本

set -e

echo "========================================"
echo "  飞书连接诊断"
echo "========================================"
echo ""

# 1. 检查 OpenFang 是否运行
echo "[1] 检查 OpenFang 进程..."
PID=$(pgrep -f "openfang" || true)
if [ -n "$PID" ]; then
    echo "✓ OpenFang 正在运行 (PID: $PID)"
else
    echo "✗ OpenFang 未运行"
    exit 1
fi
echo ""

# 2. 检查 webhook 端口
echo "[2] 检查飞书 webhook 端口 (8453)..."
if lsof -Pi :8453 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo "✓ 端口 8453 正在监听"
    lsof -Pi :8453 -sTCP:LISTEN
else
    echo "✗ 端口 8453 未监听"
fi
echo ""

# 3. 测试本地 webhook
echo "[3] 测试本地 webhook 端点..."
RESPONSE=$(curl -s -X POST http://127.0.0.1:8453/feishu/webhook \
    -H "Content-Type: application/json" \
    -d '{"challenge":"test_challenge_123","token":""}' 2>&1)
if echo "$RESPONSE" | grep -q "test_challenge_123"; then
    echo "✓ 本地 webhook 响应正常"
    echo "  响应: $RESPONSE"
else
    echo "✗ 本地 webhook 响应异常"
    echo "  响应: $RESPONSE"
fi
echo ""

# 4. 检查公网访问
echo "[4] 检查公网访问..."
PUBLIC_IP=$(curl -s https://api.ipify.org 2>/dev/null || echo "无法获取")
echo "  公网 IP: $PUBLIC_IP"
echo ""
echo "  请确保飞书开放平台的事件订阅 URL 配置正确:"
echo "  - 如果有公网 IP: http://$PUBLIC_IP:8453/feishu/webhook"
echo "  - 如果使用内网穿透: https://your-ngrok-url/feishu/webhook"
echo ""

# 5. 检查配置
echo "[5] 检查飞书配置..."
CONFIG_FILE="$HOME/.openfang/config.toml"
if [ -f "$CONFIG_FILE" ]; then
    echo "  配置文件: $CONFIG_FILE"
    if grep -q "^\[channels.feishu\]" "$CONFIG_FILE"; then
        echo "✓ 飞书配置段存在"
        APP_ID=$(grep -A 10 "^\[channels.feishu\]" "$CONFIG_FILE" | grep "app_id" | head -1 | sed 's/.*= "\(.*\)".*/\1/' || echo "未找到")
        echo "  App ID: $APP_ID"
    else
        echo "✗ 飞书配置段不存在"
    fi
else
    echo "✗ 配置文件不存在"
fi
echo ""

# 6. 检查环境变量
echo "[6] 检查环境变量..."
if [ -n "$FEISHU_APP_SECRET" ]; then
    echo "✓ FEISHU_APP_SECRET 已设置"
    echo "  值: ${FEISHU_APP_SECRET:0:10}..."
else
    echo "✗ FEISHU_APP_SECRET 未设置"
fi
echo ""

# 7. 检查日志
echo "[7] 检查飞书相关日志..."
LOG_FILE="$HOME/.openfang/daemon.log"
if [ -f "$LOG_FILE" ]; then
    echo "  最近 10 条飞书相关日志:"
    grep -i "feishu\|lark" "$LOG_FILE" 2>/dev/null | tail -10 || echo "  无飞书相关日志"
else
    echo "  日志文件不存在"
fi
echo ""

# 8. 测试模拟事件
echo "[8] 测试模拟事件..."
echo "  发送模拟消息事件到本地 webhook..."
TEST_EVENT='{
  "schema": "2.0",
  "header": {
    "event_id": "test_event_123",
    "event_type": "im.message.receive_v1",
    "create_time": "1234567890"
  },
  "event": {
    "message": {
      "message_id": "test_msg_123",
      "chat_type": "p2p",
      "chat_id": "test_chat_123",
      "message_type": "text",
      "content": "{\"text\":\"Hello from test\"}"
    },
    "sender": {
      "sender_type": "user",
      "sender_id": {
        "open_id": "test_user_123"
      }
    }
  }
}'

RESPONSE=$(curl -s -X POST http://127.0.0.1:8453/feishu/webhook \
    -H "Content-Type: application/json" \
    -d "$TEST_EVENT" 2>&1)
echo "  响应: $RESPONSE"
echo ""

echo "========================================"
echo "  诊断完成"
echo "========================================"
echo ""
echo "常见问题:"
echo "1. 如果飞书无法访问你的服务器，请检查:"
echo "   - 服务器防火墙是否开放 8453 端口"
echo "   - 如果是内网，需要使用 ngrok 等内网穿透工具"
echo ""
echo "2. 飞书开放平台配置检查:"
echo "   - 事件订阅 URL 是否正确"
echo "   - 是否订阅了 'im.message.receive_v1' 事件"
echo "   - 是否有 'im:message:send_as_bot' 权限"
echo "   - 应用是否已发布并添加到群聊"
echo ""
