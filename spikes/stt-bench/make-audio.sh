#!/bin/sh
# Writes 16 reference utterances as 16 kHz mono WAV plus manifest.tsv (id, language, path, text)
# using the macOS voices Linh (Vietnamese) and Samantha (English). Audio is not committed.
# Usage: ./make-audio.sh [output-dir]   (default: ./audio)
set -eu
out="${1:-audio}"
mkdir -p "$out"
cd "$out"
: > manifest.tsv
n=0
add() { # language voice text
  n=$((n + 1))
  id=$(printf "%s%02d" "$1" "$n")
  say -v "$2" -o "$id.aiff" "$3"
  afconvert -f WAVE -d LEI16@16000 -c 1 "$id.aiff" "$id.wav"
  rm "$id.aiff"
  printf "%s\t%s\t%s/%s.wav\t%s\n" "$id" "$1" "$PWD" "$id" "$3" >> manifest.tsv
}
V=Linh
add vi $V "Xin chào mọi người, rất vui được gặp các bạn hôm nay."
add vi $V "Bạn có thể giới thiệu ngắn gọn về kinh nghiệm làm việc của mình không?"
add vi $V "Doanh thu quý ba tăng mười hai phần trăm so với cùng kỳ năm ngoái."
add vi $V "Chúng tôi cần hoàn thành báo cáo trước thứ sáu tuần sau."
add vi $V "Hệ thống này chạy hoàn toàn trên máy tính của bạn và không gửi dữ liệu ra ngoài."
add vi $V "Tại sao anh chọn ngôn ngữ lập trình Rust cho dự án này?"
add vi $V "Cuộc họp sẽ bắt đầu lúc chín giờ ba mươi sáng tại phòng họp tầng năm."
add vi $V "Giá sản phẩm là hai triệu năm trăm nghìn đồng một tháng."
add vi $V "Xin lỗi, tôi chưa nghe rõ, bạn có thể nhắc lại câu hỏi được không?"
add vi $V "Độ trễ trung bình của hệ thống nhận dạng giọng nói là dưới một giây."
add vi $V "Chúng ta sẽ thảo luận về kế hoạch phát triển sản phẩm trong sáu tháng tới và phân công công việc cho từng thành viên trong nhóm."
add vi $V "Cảm ơn anh chị đã dành thời gian, chúng tôi sẽ liên hệ lại với kết quả trong vòng một tuần."
add vi $V "Trước hết tôi xin trình bày tổng quan về dự án, sau đó chúng ta sẽ đi vào từng hạng mục cụ thể, bao gồm tiến độ, ngân sách và những rủi ro chính mà đội ngũ đang gặp phải trong quá trình triển khai."
add en Samantha "Hello everyone, thank you for joining the call today."
add en Samantha "Could you walk me through your experience with distributed systems?"
add en Samantha "Our revenue grew twelve percent compared to the same quarter last year."
echo "wrote $n utterances to $PWD"
