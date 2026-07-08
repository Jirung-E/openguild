# 경고/확인 다이얼로그는 항상 인앱(커스텀)

GUI 에서 사용자에게 확인을 받거나 경고를 보여줄 때 브라우저/OS 네이티브
`confirm()` / `alert()` / `prompt()` 를 쓰지 말 것 — 반드시 앱 자체
컴포넌트를 쓴다.

## 이유

- Tauri WebView 의 native `confirm()`/`alert()` 는 환경에 따라 다이얼로그가
  아예 안 뜨고 조용히 return 하는 사례가 있음(DEV-118) — 사용자가 "삭제
  확인 없이 그냥 진행됐다"로 인식.
- 앱 전체 테마(다크/라이트)와 무관하게 OS 기본 스타일로 떠서 시각적으로
  붕 뜸.
- Esc/Enter 등 키보드 접근성이 컴포넌트마다 제각각.
- admin 반복 피드백(BUG-123 등) — 네이티브 다이얼로그가 뜰 때마다
  지적됨.

## 무엇을 쓸 것인가

| 용도 | 대체 |
|---|---|
| `confirm()` (예/아니오 확인) | `lib/components/ConfirmDialog.svelte` |
| `alert()` (실패/경고 메시지) | `lib/stores/toast.ts` 의 `showToast(msg, 'error')` |

```svelte
<!-- ❌ 잘못 -->
<script>
  function askDelete(id) {
    if (!confirm('삭제할까요?')) return;
    doDelete(id).catch((e) => alert(e.message));
  }
</script>

<!-- ✅ 올바름 -->
<script>
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import { showToast } from '$lib/stores/toast';

  let confirmId = $state(null);
  function askDelete(id) { confirmId = id; }
  async function doDeleteConfirmed() {
    const id = confirmId;
    confirmId = null;
    try {
      await doDelete(id);
    } catch (e) {
      showToast(e instanceof Error ? e.message : '삭제 실패', 'error');
    }
  }
</script>

<ConfirmDialog
  open={confirmId !== null}
  title="삭제"
  message={`'${confirmId ?? ''}' 를 삭제할까요?`}
  confirmLabel="삭제"
  danger
  onconfirm={doDeleteConfirmed}
  oncancel={() => (confirmId = null)}
/>
```

## 예외

없음 — 파일 선택(`@tauri-apps/plugin-dialog` 의 `open`/`save`)처럼 OS
파일시스템 자체를 다루는 네이티브 다이얼로그는 이 규칙 대상이 아님
(확인/경고 메시지 다이얼로그에 한정).
