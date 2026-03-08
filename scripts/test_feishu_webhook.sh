#!/bin/bash
# 测试飞书 webhook

echo "=== 测试飞书 Webhook ==="
echo ""

# 测试 1: URL 验证挑战
echo "[测试 1] URL 验证挑战..."
curl -s -X POST http://127.0.0.1:8453/feishu/webhook \
    -H "Content-Type: application/json" \
    -d '{
        "challenge": "test_challenge_abc123",
        "token": ""
    }' | jq .
echo ""

# 测试 2: 模拟单聊消息 (p2p)
echo "[测试 2] 模拟单聊消息 (p2p)..."
curl -s -X POST http://127.0.0.1:8453/feishu/webhook \
    -H "Content-Type: application/json" \
    -d '{
        "schema": "2.0",
        "header": {
            "event_id": "test_event_p2p_001",
            "event_type": "im.message.receive_v1",
            "create_time": "1234567890000"
        },
        "event": {
            "message": {
                "message_id": "om_test_msg_001",
                "chat_type": "p2p",
                "chat_id": "oc_test_chat_001",
                "message_type": "text",
                "content": "{\"text\":\"你好，测试消息\"}"
            },
            "sender": {
                "sender_type": "user",
                "sender_id": {
                    "open_id": "ou_test_user_001"
                }
            }
        }
    }'
echo ""
echo ""

# 测试 3: 模拟群聊消息
echo "[测试 3] 模拟群聊消息 (group)..."
curl -s -X POST http://127.0.0.1:8453/feishu/webhook \
    -H "Content-Type: application/json" \
    -d '{
        "schema": "2.0",
        "header": {
            "event_id": "test_event_group_001",
            "event_type": "im.message.receive_v1",
            "create_time": "1234567890000"
        },
        "event": {
            "message": {
                "message_id": "om_test_msg_002",
                "chat_type": "group",
                "chat_id": "oc_test_group_001",
                "message_type": "text",
                "content": "{\"text\":\"群聊测试消息\"}"
            },
            "sender": {
                "sender_type": "user",
                "sender_id": {
                    "open_id": "ou_test_user_002"
                }
            }
        }
    }'
echo ""
echo ""

# 测试 4: 模拟命令消息
echo "[测试 4] 模拟命令消息..."
curl -s -X POST http://127.0.0.1:8453/feishu/webhook \
    -H "Content-Type: application/json" \
    -d '{
        "schema": "2.0",
        "header": {
            "event_id": "test_event_cmd_001",
            "event_type": "im.message.receive_v1",
            "create_time": "1234567890000"
        },
        "event": {
            "message": {
                "message_id": "om_test_msg_003",
                "chat_type": "p2p",
                "chat_id": "oc_test_chat_003",
                "message_type": "text",
                "content": "{\"text\":\"/help\"}"
            },
            "sender": {
                "sender_type": "user",
                "sender_id": {
                    "open_id": "ou_test_user_003"
                }
            }
        }
    }'
echo ""
echo ""

echo "=== 测试完成 ==="
echo ""
echo "请检查日志查看消息处理情况:"
echo "  tail -f ~/.openfang/daemon.log | grep -i 'feishu\|coder\|channel'"
