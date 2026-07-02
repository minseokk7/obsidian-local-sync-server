<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount, tick } from "svelte";

  // ─── 상태 ────────────────────────────────────────────────────
  let status = $state<"Stopped" | "Running">("Stopped");
  let port = $state(8080);
  let syncDir = $state<string | null>(null);
  let logs = $state<string[]>([]);
  let localIps = $state<string[]>([]);
  let connectedClients = $state<{ ip: string; user_agent: string; last_seen: string }[]>([]);

  // DOM 참조
  let logContainer: HTMLDivElement | null = null;

  // 인증 설정
  let authUsername = $state("");
  let authPassword = $state("");
  let showPassword = $state(false);
  let authSaved = $state(false);

  // 복사 피드백
  let copiedIp = $state<string | null>(null);

  // ─── 동기화 폴더 선택 ─────────────────────────────────────────
  async function selectSyncDir() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "동기화할 Obsidian 볼트 폴더 선택",
      });
      if (selected && typeof selected === "string") {
        syncDir = selected;
        localStorage.setItem("syncDir", syncDir);
        addLog(`📂 동기화 폴더 설정됨: ${syncDir}`);
      }
    } catch (err) {
      addLog(`❌ 폴더 선택 실패: ${err}`);
    }
  }

  // ─── IP 복사 ─────────────────────────────────────────────────
  async function copyToClipboard(text: string) {
    try {
      await navigator.clipboard.writeText(`http://${text}:${port}/`);
      copiedIp = text;
      setTimeout(() => (copiedIp = null), 2000);
    } catch {
      // fallback
      const el = document.createElement("textarea");
      el.value = `http://${text}:${port}/`;
      document.body.appendChild(el);
      el.select();
      document.execCommand("copy");
      document.body.removeChild(el);
      copiedIp = text;
      setTimeout(() => (copiedIp = null), 2000);
    }
  }

  // ─── 서버 토글 ───────────────────────────────────────────────
  async function toggleServer() {
    if (status === "Stopped") {
      try {
        await invoke("start_server", { port, syncDir });
        status = "Running";
        addLog("✅ WebDAV 서버가 시작되었습니다.");
      } catch (err) {
        addLog(`❌ 서버 시작 실패: ${err}`);
      }
    } else {
      try {
        await invoke("stop_server");
        status = "Stopped";
        connectedClients = [];
        addLog("🛑 서버가 중지되었습니다.");
      } catch (err) {
        addLog(`❌ 서버 중지 실패: ${err}`);
      }
    }
  }

  // ─── 인증 저장 ───────────────────────────────────────────────
  async function saveCredentials() {
    try {
      await invoke("set_credentials", {
        username: authUsername,
        password: authPassword,
      });
      
      localStorage.setItem("authUsername", authUsername);
      localStorage.setItem("authPassword", authPassword);

      authSaved = true;
      addLog(
        authUsername.trim()
          ? `🔐 인증 설정 저장됨 (사용자: ${authUsername})`
          : "🔓 인증이 해제되었습니다."
      );
      setTimeout(() => (authSaved = false), 2000);
    } catch (err) {
      addLog(`❌ 인증 설정 실패: ${err}`);
    }
  }

  // ─── 접속 기기 갱신 ─────────────────────────────────────────
  async function refreshClients() {
    try {
      connectedClients = await invoke("get_connected_clients");
    } catch {}
  }

  async function addLog(msg: string) {
    const time = new Date().toLocaleTimeString("ko-KR");
    // 새 로그를 배열 끝에 추가
    logs = [...logs, `[${time}] ${msg}`].slice(-200);
    await tick();
    if (logContainer) {
      logContainer.scrollTop = logContainer.scrollHeight;
    }
  }

  // ─── 기기명 파싱 (User-Agent 간소화) ────────────────────────
  function parseDevice(ua: string): string {
    if (ua === "Unknown") return "알 수 없는 기기";
    if (/android/i.test(ua)) {
      const model = ua.match(/\(Linux; Android [^;]+; ([^)]+)\)/)?.[1];
      return model ? `Android — ${model}` : "Android 기기";
    }
    if (/iphone/i.test(ua)) return "iPhone";
    if (/ipad/i.test(ua)) return "iPad";
    if (/obsidian/i.test(ua)) return "Obsidian App";
    if (/windows/i.test(ua)) return "Windows PC";
    if (/mac/i.test(ua)) return "Mac";
    return ua.length > 40 ? ua.slice(0, 40) + "…" : ua;
  }

  // ─── 마운트 ──────────────────────────────────────────────────
  onMount(async () => {
    syncDir = localStorage.getItem("syncDir");
    
    // 저장된 인증 정보 불러오기
    const savedUser = localStorage.getItem("authUsername");
    const savedPass = localStorage.getItem("authPassword");
    if (savedUser) authUsername = savedUser;
    if (savedPass) authPassword = savedPass;

    // 로컬 IP 조회
    try {
      localIps = await invoke("get_local_ips");
    } catch {}

    // 서버 로그 수신
    const unlistenLog = listen<string>("server-log", (e) => addLog(e.payload));

    // 서버 에러 수신
    const unlistenError = listen<string>("server-error", (e) => {
      addLog(`❌ ${e.payload}`);
      status = "Stopped";
    });

    // 새 기기 접속 이벤트 수신
    const unlistenClients = listen("clients-updated", () => refreshClients());

    // 주기적 클라이언트 갱신 (30초)
    const interval = setInterval(refreshClients, 30_000);

    return () => {
      unlistenLog.then((f) => f());
      unlistenError.then((f) => f());
      unlistenClients.then((f) => f());
      clearInterval(interval);
    };
  });
</script>

<main
  class="min-h-screen p-6 flex flex-col items-center relative overflow-hidden bg-gradient-to-br from-neutral-900 via-neutral-800 to-black"
>
  <!-- 배경 블러 블롭 -->
  <div
    class="absolute top-1/4 left-1/4 w-96 h-96 bg-blue-600/20 rounded-full mix-blend-screen filter blur-[140px] animate-pulse"
  ></div>
  <div
    class="absolute bottom-1/4 right-1/4 w-96 h-96 bg-emerald-600/20 rounded-full mix-blend-screen filter blur-[140px] animate-pulse"
    style="animation-delay: 2s;"
  ></div>

  <div class="w-full max-w-5xl flex flex-col gap-5 relative z-10">

    <!-- ── 헤더 ────────────────────────────────────────────────── -->
    <header class="flex justify-between items-center">
      <div>
        <h1
          class="text-3xl font-bold bg-gradient-to-r from-blue-400 to-emerald-400 bg-clip-text text-transparent"
        >
          Obsidian Local Sync
        </h1>
        <p class="text-neutral-500 mt-1 text-sm">
          Remotely Save 플러그인용 로컬 WebDAV 서버
        </p>
      </div>
      <!-- 서버 상태 뱃지 -->
      <div
        class="flex items-center gap-2 px-4 py-2 rounded-full border {status === 'Running'
          ? 'border-emerald-500/40 bg-emerald-500/10 text-emerald-400'
          : 'border-neutral-700 bg-neutral-800/60 text-neutral-400'}"
      >
        <span
          class="w-2.5 h-2.5 rounded-full {status === 'Running'
            ? 'bg-emerald-400 animate-pulse'
            : 'bg-neutral-600'}"
        ></span>
        <span class="text-sm font-semibold">
          {status === "Running" ? "서버 실행 중" : "서버 중지됨"}
        </span>
      </div>
    </header>

    <!-- ── 상단 2열 그리드 ─────────────────────────────────────── -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-5">

      <!-- 서버 제어 패널 -->
      <div class="bg-neutral-900/60 border border-white/8 rounded-2xl p-5 flex flex-col gap-4 backdrop-blur-sm">
        <h2 class="text-sm font-semibold text-neutral-300 flex items-center gap-2">
          <svg class="w-4 h-4 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
              d="M5 12h14M12 5l7 7-7 7" />
          </svg>
          서버 설정
        </h2>

        <!-- 포트 입력 -->
        <div class="flex flex-col gap-1.5">
          <label for="port" class="text-xs text-neutral-500 font-medium">포트 번호</label>
          <input
            id="port"
            type="number"
            bind:value={port}
            disabled={status === "Running"}
            class="bg-black/40 border border-white/10 rounded-xl px-4 py-2.5 text-white text-sm
                   focus:outline-none focus:ring-2 focus:ring-blue-500 transition-all
                   disabled:opacity-40 disabled:cursor-not-allowed"
          />
        </div>

        <!-- 동기화 폴더 선택 -->
        <div class="flex flex-col gap-1.5">
          <p class="text-xs text-neutral-500 font-medium">동기화할 대상 폴더</p>
          <div class="flex gap-2">
            <div
              class="flex-1 bg-black/40 border border-white/10 rounded-xl px-3 py-2.5 text-xs text-neutral-300 truncate flex items-center {status === 'Running' ? 'opacity-40' : ''}"
              title={syncDir || "기본 데이터 폴더 사용"}
            >
              {syncDir || "기본 임시 폴더 사용 중..."}
            </div>
            <button
              onclick={selectSyncDir}
              disabled={status === "Running"}
              class="shrink-0 px-4 py-2 bg-white/10 hover:bg-white/15 text-white text-xs font-medium rounded-xl transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
            >
              폴더 변경
            </button>
          </div>
        </div>

        <!-- 시작/중지 버튼 -->
        <button
          onclick={toggleServer}
          class="w-full py-3 rounded-xl font-bold text-sm transition-all duration-200 active:scale-95
                 {status === 'Running'
                   ? 'bg-red-500/15 text-red-400 hover:bg-red-500/25 border border-red-500/40'
                   : 'bg-blue-600 hover:bg-blue-500 text-white shadow-[0_0_24px_rgba(37,99,235,0.35)]'}"
        >
          {status === "Running" ? "⏹ 서버 중지" : "▶ 서버 시작"}
        </button>

        <!-- 로컬 IP 표시 -->
        <div class="flex flex-col gap-2">
          <p class="text-xs text-neutral-500 font-medium">이 PC의 주소 (플러그인에 입력)</p>
          {#if localIps.length === 0}
            <p class="text-xs text-neutral-600 italic">IP를 불러오는 중...</p>
          {:else}
            {#each localIps as ip}
              <div class="flex items-center gap-2 bg-black/30 rounded-lg px-3 py-2 border border-white/5">
                <code class="text-emerald-400 text-xs flex-1 select-all">
                  http://{ip}:{port}/
                </code>
                <button
                  onclick={() => copyToClipboard(ip)}
                  class="text-xs px-2.5 py-1 rounded-md transition-all duration-150
                         {copiedIp === ip
                           ? 'bg-emerald-500/20 text-emerald-400'
                           : 'bg-white/5 text-neutral-400 hover:bg-white/10 hover:text-white'}"
                  title="클립보드에 복사"
                >
                  {copiedIp === ip ? "✓ 복사됨" : "복사"}
                </button>
              </div>
            {/each}
          {/if}
        </div>
      </div>

      <!-- 인증 설정 패널 -->
      <div class="bg-neutral-900/60 border border-white/8 rounded-2xl p-5 flex flex-col gap-4 backdrop-blur-sm">
        <h2 class="text-sm font-semibold text-neutral-300 flex items-center gap-2">
          <svg class="w-4 h-4 text-amber-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
              d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
          </svg>
          인증 설정 (Basic Auth)
        </h2>
        <p class="text-xs text-neutral-600 -mt-2">
          비워두면 인증 없이 누구나 접속 가능합니다.
        </p>

        <!-- 아이디 -->
        <div class="flex flex-col gap-1.5">
          <label for="username" class="text-xs text-neutral-500 font-medium">아이디</label>
          <input
            id="username"
            type="text"
            bind:value={authUsername}
            placeholder="(없음)"
            autocomplete="off"
            class="bg-black/40 border border-white/10 rounded-xl px-4 py-2.5 text-white text-sm
                   placeholder-neutral-700 focus:outline-none focus:ring-2 focus:ring-amber-500 transition-all"
          />
        </div>

        <!-- 비밀번호 -->
        <div class="flex flex-col gap-1.5">
          <label for="password" class="text-xs text-neutral-500 font-medium">비밀번호</label>
          <div class="relative">
            <input
              id="password"
              type={showPassword ? "text" : "password"}
              bind:value={authPassword}
              placeholder="(없음)"
              autocomplete="new-password"
              class="w-full bg-black/40 border border-white/10 rounded-xl px-4 py-2.5 text-white text-sm
                     placeholder-neutral-700 focus:outline-none focus:ring-2 focus:ring-amber-500 transition-all pr-10"
            />
            <button
              type="button"
              onclick={() => (showPassword = !showPassword)}
              class="absolute right-3 top-1/2 -translate-y-1/2 text-neutral-500 hover:text-white transition-colors"
            >
              {showPassword ? "🙈" : "👁️"}
            </button>
          </div>
        </div>

        <!-- 저장 버튼 -->
        <button
          onclick={saveCredentials}
          class="w-full py-2.5 rounded-xl font-semibold text-sm transition-all duration-200 active:scale-95
                 {authSaved
                   ? 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/40'
                   : 'bg-amber-600/80 hover:bg-amber-500 text-white'}"
        >
          {authSaved ? "✓ 저장됨" : "인증 설정 저장"}
        </button>
      </div>
    </div>

    <!-- ── 접속 기기 목록 ───────────────────────────────────────── -->
    <div class="bg-neutral-900/60 border border-white/8 rounded-2xl p-5 flex flex-col gap-3 backdrop-blur-sm">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold text-neutral-300 flex items-center gap-2">
          <svg class="w-4 h-4 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
              d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z" />
          </svg>
          접속 기기
          {#if connectedClients.length > 0}
            <span class="ml-1 px-2 py-0.5 rounded-full text-xs bg-purple-500/20 text-purple-400 border border-purple-500/30">
              {connectedClients.length}
            </span>
          {/if}
        </h2>
        <button
          onclick={refreshClients}
          class="text-xs text-neutral-500 hover:text-white transition-colors px-2 py-1 rounded-lg hover:bg-white/5"
        >
          새로고침
        </button>
      </div>

      {#if connectedClients.length === 0}
        <div class="text-center py-6 text-neutral-700 text-sm italic">
          아직 접속한 기기가 없습니다.
        </div>
      {:else}
        <div class="divide-y divide-white/5">
          {#each connectedClients as client}
            <div class="flex items-center gap-4 py-3">
              <div class="w-9 h-9 rounded-xl bg-purple-500/15 border border-purple-500/20 flex items-center justify-center text-lg">
                {#if /android/i.test(client.user_agent)}📱
                {:else if /iphone|ipad/i.test(client.user_agent)}🍎
                {:else if /windows/i.test(client.user_agent)}🖥️
                {:else if /mac/i.test(client.user_agent)}💻
                {:else}📡{/if}
              </div>
              <div class="flex-1 min-w-0">
                <p class="text-sm font-medium text-neutral-200 truncate">
                  {parseDevice(client.user_agent)}
                </p>
                <p class="text-xs text-neutral-600 mt-0.5">
                  {client.ip} · 마지막 접속: {client.last_seen}
                </p>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <!-- ── 서버 로그 ──────────────────────────────────────────── -->
    <div class="bg-neutral-900/60 border border-white/8 rounded-2xl p-5 flex flex-col gap-3 backdrop-blur-sm">
      <div class="flex items-center justify-between">
        <h2 class="text-sm font-semibold text-neutral-300 flex items-center gap-2">
          <svg class="w-4 h-4 text-neutral-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h7" />
          </svg>
          서버 로그
        </h2>
        <button
          onclick={() => (logs = [])}
          class="text-xs text-neutral-600 hover:text-red-400 transition-colors px-2 py-1 rounded-lg hover:bg-red-500/5"
        >
          지우기
        </button>
      </div>

      <div bind:this={logContainer} class="h-52 overflow-y-auto font-mono text-xs space-y-1 custom-scrollbar">
        {#if logs.length === 0}
          <div class="text-neutral-700 italic mt-2">서버를 시작하면 로그가 여기에 표시됩니다.</div>
        {:else}
          {#each logs as log}
            <div
              class="py-0.5 border-b border-white/[0.03]
                     {log.includes('❌') ? 'text-red-400' :
                      log.includes('✅') || log.includes('🔌') ? 'text-emerald-400' :
                      'text-neutral-400'}"
            >
              {log}
            </div>
          {/each}
        {/if}
      </div>
    </div>

  </div>
</main>

<style>
  .custom-scrollbar::-webkit-scrollbar { width: 4px; }
  .custom-scrollbar::-webkit-scrollbar-track { background: transparent; }
  .custom-scrollbar::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.08);
    border-radius: 4px;
  }
  .custom-scrollbar::-webkit-scrollbar-thumb:hover {
    background: rgba(255, 255, 255, 0.15);
  }
</style>
