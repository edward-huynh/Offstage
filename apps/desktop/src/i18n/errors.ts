import { getLanguage, t, type Key } from ".";

/**
 * Messages the Rust side sends in Vietnamese, with the text of each in the dictionaries. Rust
 * cannot know the language of the screen, so the match is made here, on the message as it is
 * shown. `(?<detail>…)` is the part of a message that follows it (an operating system error, a
 * reason from another message), translated too when it is one of these.
 */
const MESSAGES: Array<[RegExp, Key]> = [
  [/^Có lỗi không xác định$/, "error.unknown"],
  [/^Phải còn ít nhất một workspace$/, "error.lastWorkspace"],
  [/^Tên workspace không được để trống$/, "error.emptyWorkspaceName"],
  [/^Không hiểu tổ hợp phím này$/, "error.hotkey.unknown"],
  [/^Tổ hợp phím cần có Ctrl, Alt hoặc ⌘ \(Super\) đi kèm$/, "error.hotkey.modifier"],
  [/^Tổ hợp phím này đã dùng cho một hành động khác$/, "error.hotkey.taken"],
  [/^Không đăng ký được tổ hợp phím này: có thể một ứng dụng khác đang dùng nó$/, "error.hotkey.register"],
  [/^Không mở được tệp: (?<detail>[\s\S]*)$/, "error.file.open"],
  [/^Không đọc được tệp: (?<detail>[\s\S]*)$/, "error.file.read"],
  [/^Không phải tệp$/, "error.file.notFile"],
  [/^\(không rõ tên\)$/, "error.file.unknownName"],
  [/^Chỉ nhận \.pdf, \.docx, \.md, \.txt$/, "error.doc.type"],
  [/^Tệp quá lớn \(tối đa 20 MB\)$/, "error.doc.tooLarge"],
  [/^Tệp rỗng$/, "error.doc.empty"],
  [/^Chưa có nội dung$/, "error.doc.noText"],
  [/^Chỉ tạo được tài liệu \.md hoặc \.txt$/, "error.doc.textKind"],
  [/^Chưa tải model chạy trên máy\. Mở Cài đặt, mục Trợ lý AI, để tải về\.$/, "error.ai.noLocalModel"],
  [/^Tải model chạy trên máy trước khi bật chế độ chỉ chạy trên máy\.$/, "error.ai.localOnlyNeedsModel"],
  [/^Chỉ nhập được tệp \.txt hoặc \.md$/, "error.script.type"],
  [/^Tệp quá lớn để làm kịch bản \(tối đa 1 MB\)$/, "error.script.tooLarge"],
  [/^Không đọc được tệp, cần mã hoá UTF-8$/, "error.script.utf8"],
  [/^Tệp không có chữ để đọc\. Nếu là PDF quét từ ảnh thì cần một bản có chữ\.$/, "error.extract.noText"],
  [/^Không đọc được nội dung tệp: (?<detail>[\s\S]*)$/, "error.extract.unreadable"],
  [/^tệp PDF bị hỏng$/, "error.extract.pdfBroken"],
  [/^không phải tệp DOCX$/, "error.extract.notDocx"],
  [/^cannot read the document: tệp quá lớn$/, "error.extract.tooLarge"],
];

/**
 * The message in the language of the screen. A message in Vietnamese is shown as it is when the
 * screen is in Vietnamese, and one that is not known (an English error from a library, say) is
 * shown as it is in either.
 */
export function translateError(message: string): string {
  if (getLanguage() === "vi") return message;
  for (const [pattern, key] of MESSAGES) {
    const match = pattern.exec(message);
    if (!match) continue;
    const detail = match.groups?.detail;
    return t(key, detail === undefined ? undefined : { detail: translateError(detail) });
  }
  return message;
}
