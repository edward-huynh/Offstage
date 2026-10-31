import { useState } from "react";
import { Check, Cloud, Loader2, Sparkles } from "lucide-react";
import { Button } from "@/components/motion/button";
import { Input } from "@/components/motion/input";
import { RadioGroup, RadioGroupItem } from "@/components/motion/radio";
import { Switch } from "@/components/motion/switch";
import { useAssistantSettings } from "../useAssistant";
import { ErrorNote, Row, Section } from "./ui";

const CUSTOM = "custom";

const PROVIDERS: { id: string; label: string }[] = [
  { id: "openai", label: "OpenAI" },
  { id: "opencode-zen", label: "OpenCode Zen" },
  { id: "opencode-go", label: "OpenCode Go" },
  { id: CUSTOM, label: "Tùy chỉnh" },
];

const SELECT_CLASS =
  "h-9 max-w-64 rounded-lg border border-border bg-background px-2.5 text-sm text-foreground";

/** Turns the assistant on, chooses where its answers come from, and keeps the key. */
export function AssistantSettings() {
  const a = useAssistantSettings();
  const [asking, setAsking] = useState(false);
  const [key, setKey] = useState("");
  // What is being typed, until the field loses focus: a half-typed address is not worth saving.
  const [address, setAddress] = useState<string | null>(null);
  const [model, setModel] = useState<string | null>(null);

  const { config, status } = a;
  if (!config || !status) {
    return (
      <Section title="Trợ lý AI" icon={<Sparkles className="size-4.5" />}>
        {a.error ? (
          <ErrorNote className="my-3">{a.error}</ErrorNote>
        ) : (
          <p className="flex items-center gap-2 py-3.5 text-sm text-muted-foreground">
            <Loader2 className="size-4 animate-spin" />
            Đang tải…
          </p>
        )}
      </Section>
    );
  }

  const providerLabel = PROVIDERS.find((p) => p.id === config.provider_id)?.label ?? config.provider_id;
  const isCustom = config.provider_id === CUSTOM;
  const modelValue = model ?? config.model ?? "";

  const commitAddress = () => {
    if (address !== null && address !== (config.custom_base_url ?? "")) {
      void a.change({ custom_base_url: address.trim() || null });
    }
    setAddress(null);
  };
  const commitModel = (value: string) => {
    setModel(null);
    if (value.trim() !== (config.model ?? "")) void a.change({ model: value.trim() || null });
  };

  let state: string;
  if (status.ready) {
    state = status.cloud
      ? `Đang bật. Câu hỏi của đối phương sẽ được gửi tới ${providerLabel} để gợi ý trả lời.`
      : "Đang bật. Mọi thứ chạy trên máy bạn.";
  } else if (status.enabled) {
    state = status.needs_key && !status.has_key ? "Đã bật nhưng chưa có khoá API." : "Đã bật nhưng chưa chọn model.";
  } else {
    state = "Đang tắt.";
  }

  return (
    <Section
      title="Trợ lý AI"
      hint="Nghe câu hỏi của đối phương và gợi ý vài gạch đầu dòng để bạn trả lời."
      icon={<Sparkles className="size-4.5" />}
    >
      <Row label="Bật trợ lý" hint={state}>
        {status.cloud && config.enabled && (
          <span
            className="inline-flex items-center gap-1.5 rounded-full bg-muted px-2.5 py-1 text-xs font-semibold text-muted-foreground"
            title="Chữ được gửi tới nhà cung cấp qua mạng"
          >
            <Cloud className="size-3.5" />
            Qua mạng
          </span>
        )}
        <Switch
          label="Bật trợ lý AI"
          checked={config.enabled}
          onCheckedChange={(on) => {
            if (on) setAsking(true);
            else {
              setAsking(false);
              void a.change({ enabled: false });
            }
          }}
        />
      </Row>

      {asking && !config.enabled && (
        <div className="my-3 flex flex-col gap-3 rounded-xl border border-border bg-muted/50 p-3.5">
          <p className="text-sm leading-relaxed text-foreground">
            Khi bật, câu hỏi và vài dòng lời thoại gần đó (cả của bạn nếu bật micro) được gửi dưới dạng chữ tới{" "}
            <strong>{providerLabel}</strong> để tạo câu trả lời. Âm thanh không bao giờ rời máy bạn.
          </p>
          <div className="flex justify-end gap-2">
            <Button variant="ghost" size="sm" onClick={() => setAsking(false)}>
              Huỷ
            </Button>
            <Button
              size="sm"
              onClick={() => {
                setAsking(false);
                void a.change({ enabled: true });
              }}
            >
              Đồng ý và bật
            </Button>
          </div>
        </div>
      )}

      <Row label="Nhà cung cấp" hint="Chỉ cần một khoá API. OpenCode Zen và Go dùng chung một khoá.">
        <RadioGroup
          orientation="horizontal"
          value={config.provider_id}
          onValueChange={(id) => void a.changeProvider(id)}
          aria-label="Nhà cung cấp"
        >
          {PROVIDERS.map((p) => (
            <RadioGroupItem key={p.id} value={p.id} label={p.label} />
          ))}
        </RadioGroup>
      </Row>

      {isCustom && (
        <Row
          label="Địa chỉ endpoint"
          hint="Một địa chỉ tương thích OpenAI, như https://openrouter.ai/api/v1 hoặc http://localhost:11434/v1 cho Ollama. http chỉ dùng được với localhost."
        >
          <Input
            className="w-72"
            aria-label="Địa chỉ endpoint"
            placeholder="https://…/v1"
            value={address ?? config.custom_base_url ?? ""}
            onChange={setAddress}
            onBlur={commitAddress}
          />
        </Row>
      )}

      <Row
        label="Khoá API"
        hint="Lưu trong Keychain của hệ điều hành và không hiện lại sau khi lưu. Chỉ phần Rust của app dùng khoá."
      >
        {status.has_key ? (
          <>
            <span className="inline-flex items-center gap-1.5 rounded-full bg-success/10 px-2.5 py-1 text-xs font-semibold text-success">
              <Check className="size-3.5" />
              Đã lưu
            </span>
            <Button variant="ghost" size="sm" disabled={a.busy === "key"} onClick={() => void a.removeKey()}>
              Xoá khoá
            </Button>
          </>
        ) : (
          <>
            <Input
              className="w-56"
              type="password"
              autoComplete="off"
              aria-label="Khoá API"
              placeholder={isCustom ? "Bỏ trống nếu không cần" : "sk-…"}
              value={key}
              onChange={setKey}
            />
            <Button
              size="sm"
              disabled={!key.trim() || a.busy === "key"}
              onClick={() => {
                void a.saveKey(key).then(() => setKey(""));
              }}
            >
              Lưu
            </Button>
          </>
        )}
      </Row>

      <Row label="Model" hint="Tải danh sách từ nhà cung cấp, hoặc gõ tên model.">
        {a.models.length > 0 ? (
          <select
            className={SELECT_CLASS}
            aria-label="Model"
            value={modelValue}
            onChange={(e) => commitModel(e.target.value)}
          >
            {modelValue === "" && <option value="">Chọn model…</option>}
            {!a.models.includes(modelValue) && modelValue !== "" && <option value={modelValue}>{modelValue}</option>}
            {a.models.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
        ) : (
          <Input
            className="w-56"
            aria-label="Model"
            placeholder="gpt-4o-mini"
            value={modelValue}
            onChange={setModel}
            onBlur={() => commitModel(modelValue)}
          />
        )}
        <Button
          variant="secondary"
          size="sm"
          disabled={a.busy === "models" || (status.needs_key && !status.has_key)}
          onClick={() => void a.loadModels()}
        >
          {a.busy === "models" ? "Đang tải…" : "Tải danh sách"}
        </Button>
      </Row>

      <Row label="Kiểm tra kết nối" hint="Gọi nhà cung cấp một lần để thử địa chỉ và khoá.">
        {a.connection === "ok" && (
          <span className="inline-flex items-center gap-1.5 text-xs font-semibold text-success">
            <Check className="size-3.5" />
            Kết nối được
          </span>
        )}
        <Button
          variant="secondary"
          size="sm"
          disabled={a.busy === "test" || (status.needs_key && !status.has_key)}
          onClick={() => void a.test()}
        >
          {a.busy === "test" ? "Đang thử…" : "Thử kết nối"}
        </Button>
      </Row>

      {a.error && <ErrorNote className="my-3">{a.error}</ErrorNote>}
    </Section>
  );
}
