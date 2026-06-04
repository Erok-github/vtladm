pub(super) const HOME_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — 首页</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
.card-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(220px,1fr));gap:1rem;}
.card{background:var(--card);border:1px solid var(--border);border-radius:10px;padding:1rem;text-decoration:none;color:inherit;display:block;box-shadow:0 1px 2px rgba(0,0,0,.04);}
.card:hover{box-shadow:0 4px 14px rgba(0,0,0,.08);} .card h3{margin:0 0 .35rem;font-size:1rem;} .card p{margin:0;font-size:.84rem;color:var(--muted);line-height:1.4;}
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_vp_side_inner.html"),
    r#"</aside>
<main class="vp-main vp-workspace">
<div class="breadcrumb">存储 · 虚拟磁带库 <b>›</b> <b>控制台</b></div>
<div class="topbar"><div><h1 style="margin:0 0 .35rem;font-size:1.28rem;">VTL 控制台</h1>
<p style="margin:0;color:var(--muted);font-size:.9rem;max-width:40rem;">为备份软件提供虚拟磁带库存储层：Web 负责建库、建磁带、SCSI/iSCSI/FC 链路；备份侧发现带库并读写磁带。</p></div></div>
<div class="card-grid">
<a class="card" href="/admin/library"><h3>① 磁带库</h3><p>创建在线库、驱动器/槽位几何，对齐内核 vtl。</p></a>
<a class="card" href="/admin/tapes"><h3>② 磁带与槽位</h3><p>批量建带、入槽、出库、inventory 对账。</p></a>
<a class="card" href="/admin/transport"><h3>③ 传输链路</h3><p>local / iSCSI / FC 向导与配置检查。</p></a>
<a class="card" href="/browse/tapes"><h3>磁带目录</h3><p>只读浏览条码、容量与位置。</p></a>
<a class="card" href="/browse/status"><h3>库状态</h3><p>全部库汇总与按库 JSON。</p></a>
<a class="card" href="/admin/overview"><h3>后台概览</h3><p>分层入口与运维快捷链接。</p></a>
</div>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"
document.getElementById('lo').onclick=async(ev)=>{ev.preventDefault();await fetch('/api/logout',{method:'POST',credentials:'include'});location.href='/login';};
</script></body></html>
"#
);

pub(super) const BROWSE_TAPES_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — 磁带目录</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_vp_side_inner.html"),
    r#"</aside>
<main class="vp-main vp-workspace">
<div class="breadcrumb">存储功能 <b>›</b> 虚拟磁带库 <b>›</b> <b>磁带目录</b></div>
<div class="topbar"><h1 style="margin:0;font-size:1.22rem;">磁带目录</h1><span class="hint" style="margin:0">条码 · 容量 · 已用 · 位置</span></div>
<div class="toolbar">
<span class="inline"><label style="display:inline;margin:0">在线库</label> <select id="lib"></select>
<button type="button" id="reload">刷新</button></span>
</div>
<section class="panel"><div id="tapes"></div></section>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"function fmtBytes(n){
  n=Number(n)||0;if(n===0)return'0 B';
  const u=['B','KB','MB','GB','TB'];let i=0,x=n;
  while(x>=1024&&i<u.length-1){x/=1024;i++;}
  return (x>=100||i===0?x.toFixed(0):x.toFixed(1))+' '+u[i];
}
function escapeHtml(s){return String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
function loc(x){
  if(x.in_drive)return'驱动器';
  if(x.slot!=null)return'槽位 '+x.slot;
  if(x.shelf_name)return'架: '+escapeHtml(x.shelf_name);
  return'货架';
}
async function loadLibs(){
  const r=await fetch('/api/libraries',{credentials:'include'});
  const j=await r.json();
  if(!r.ok){document.getElementById('tapes').innerHTML='<p class="err">'+(j.error||r.status)+'</p>';return;}
  const sel=document.getElementById('lib');
  sel.innerHTML='';
  (j.libraries||[]).filter(l=>!l.is_offline_storage&&l.name).forEach(l=>{const o=document.createElement('option');o.value=l.name;o.textContent=l.name+' (#'+l.id+')';sel.appendChild(o);});
  if(sel.options.length) await refresh();
}
async function refresh(){
  const lib=document.getElementById('lib').value;
  const t=await fetch('/api/tapes?library='+encodeURIComponent(lib),{credentials:'include'});
  const tj=await t.json();
  if(!t.ok){document.getElementById('tapes').innerHTML='<p class="err">'+(tj.error||t.status)+'</p>';return;}
  const rows=tj.tapes||[];
  if(!rows.length){document.getElementById('tapes').innerHTML='<div class="empty">当前库暂无磁带</div>';return;}
  let h='<table class="data-table"><thead><tr><th>名称</th><th>条码</th><th class="num">容量</th><th class="num">已用</th><th>位置</th><th>货架</th><th>在驱动</th></tr></thead><tbody>';
  rows.forEach(x=>{h+='<tr><td>'+escapeHtml(x.name)+'</td><td>'+escapeHtml(x.barcode)+'</td><td class="num">'+fmtBytes(x.capacity_bytes)+'</td><td class="num">'+fmtBytes(x.used_bytes||0)+'</td><td>'+loc(x)+'</td><td>'+(x.shelf_name?escapeHtml(x.shelf_name):'—')+'</td><td>'+(x.in_drive?'是':'否')+'</td></tr>';});
  if(tj.truncated){h+='<tr><td colspan="7" style="text-align:center;color:#888">（已显示前 '+rows.length+' 条，共 '+tj.total+' 条）</td></tr>';}
  h+='</tbody></table>';document.getElementById('tapes').innerHTML=h;
}
document.getElementById('reload').onclick=refresh;
document.getElementById('lib').onchange=refresh;
loadLibs();
document.getElementById('lo').onclick=async(ev)=>{ev.preventDefault();await fetch('/api/logout',{method:'POST',credentials:'include'});location.href='/login';};
</script></body></html>
"#
);

pub(super) const BROWSE_STATUS_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — 库状态</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_vp_side_inner.html"),
    r#"</aside>
<main class="vp-main vp-workspace">
<div class="breadcrumb">存储功能 <b>›</b> 虚拟磁带库 <b>›</b> <b>库状态</b></div>
<div class="topbar"><h1 style="margin:0;font-size:1.22rem;">库状态</h1></div>
<section class="panel">
<h2 style="margin:0 0 .6rem;font-size:1rem;border:0;padding:0">全部库汇总</h2>
<div id="sum-wrap"></div>
<button type="button" id="reload-sum">刷新汇总</button>
</section>
<section class="panel">
<label>当前库（详细 JSON）</label> <select id="lib"></select>
<button type="button" id="reload">刷新详情</button>
</section>
<section class="panel"><pre id="status"></pre></section>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"function escapeHtml(s){return String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
async function loadSummary(){
  const r=await fetch('/api/libraries-status',{credentials:'include'});
  const j=await r.json();
  const el=document.getElementById('sum-wrap');
  if(!r.ok){el.innerHTML='<p class="err">'+(j.error||r.status)+'</p>';return;}
  const rows=j.libraries||[];
  if(!rows.length){el.innerHTML='<div class="empty">暂无库</div>';return;}
  let h='<table class="data-table"><thead><tr><th>库</th><th class="num">磁带数</th><th class="num">已加载驱动</th><th class="num">驱动数</th><th class="num">数据槽位</th><th>iSCSI</th></tr></thead><tbody>';
  rows.forEach(x=>{
    const isc=x.iscsi_exported===true?('是 '+escapeHtml(x.iscsi_iqn||'')):(x.iscsi_exported===false?'否':'—');
    h+='<tr><td>'+escapeHtml(x.library)+'</td><td class="num">'+x.tape_count+'</td><td class="num">'+x.loaded_in_drives+'</td><td class="num">'+x.drives+'</td><td class="num">'+x.data_slots+'</td><td>'+isc+'</td></tr>';
  });
  h+='</tbody></table>';el.innerHTML=h;
}
async function loadLibs(){
  const r=await fetch('/api/libraries',{credentials:'include'});
  const j=await r.json();
  if(!r.ok){document.getElementById('status').textContent=(j.error||r.status);return;}
  const sel=document.getElementById('lib');sel.innerHTML='';
  (j.libraries||[]).forEach(l=>{const o=document.createElement('option');o.value=l.name;o.textContent=l.name+' (#'+l.id+')';sel.appendChild(o);});
  if(sel.options.length) await refresh();
}
async function refresh(){
  const lib=document.getElementById('lib').value;
  const s=await fetch('/api/status?library='+encodeURIComponent(lib),{credentials:'include'});
  const sj=await s.json();
  document.getElementById('status').textContent=JSON.stringify(sj,null,2);
}
document.getElementById('reload-sum').onclick=loadSummary;
document.getElementById('reload').onclick=refresh;
document.getElementById('lib').onchange=refresh;
loadSummary();
loadLibs();
document.getElementById('lo').onclick=async(ev)=>{ev.preventDefault();await fetch('/api/logout',{method:'POST',credentials:'include'});location.href='/login';};
</script></body></html>
"#
);

pub(super) const BROWSE_FABRIC_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — 传输与路径</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_vp_side_inner.html"),
    r#"</aside>
<main class="vp-main vp-workspace">
<div class="breadcrumb">存储功能 <b>›</b> 虚拟磁带库 <b>›</b> <b>传输配置</b></div>
<div class="topbar"><h1 style="margin:0;font-size:1.22rem;">传输配置（JSON）</h1>
<p class="hint" style="margin:.35rem 0 0">运维只读；分层说明与 iSCSI 操作请用 <a href="/admin/transport">传输向导</a>、<a href="/admin/iscsi">iSCSI 映射</a>。</p></div>
<section class="panel"><button type="button" id="reload">刷新</button><pre id="out"></pre></section>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"async function refresh(){
  const r=await fetch('/api/fabric',{credentials:'include'});
  const j=await r.json();
  document.getElementById('out').textContent=r.ok?JSON.stringify(j,null,2):JSON.stringify(j,null,2);
}
document.getElementById('reload').onclick=refresh;
refresh();
document.getElementById('lo').onclick=async(ev)=>{ev.preventDefault();await fetch('/api/logout',{method:'POST',credentials:'include'});location.href='/login';};
</script></body></html>
"#
);

pub(super) const ADMIN_SETUP_INIT_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL 初始化配置</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
body.login-page{display:flex;min-height:100vh;align-items:flex-start;justify-content:center;padding:1.25rem;margin:0;}
.setup-card{width:100%;max-width:560px;background:var(--card);border:1px solid var(--border);border-radius:10px;padding:1.25rem 1.35rem;box-shadow:0 4px 20px rgba(15,23,42,.08);margin-top:1rem;}
.setup-card h1{margin:0 0 .35rem;font-size:1.15rem;color:var(--accent);}
.setup-hint{font-size:.84rem;color:var(--muted);line-height:1.55;margin:0 0 1rem;}
.setup-card label{display:block;margin:.45rem 0 .15rem;font-size:.88rem;}
.setup-card input[type=text]{width:100%;box-sizing:border-box;}
.setup-card button{margin-top:1rem;padding:.55rem 1rem;font-weight:600;}
.setup-card .err{margin:.5rem 0 0;color:#b91c1c;}
.setup-done{display:none;margin-top:.75rem;}
</style>
</head>
<body class="login-page">
<div class="setup-card">
<h1>首次初始化配置</h1>
<p class="setup-hint">未检测到 <code>/opt/vtladm/var/vtl.conf</code>。路径须为<strong>绝对路径</strong>；留空则使用 <code>/opt/vtladm/var/</code> 下默认路径（库、磁带镜像、日志）。提交后将创建目录并写入主配置。默认 <strong>不在提交瞬间</strong> 重载内核；改库后默认走 <code>vtl-kernelctl reload</code>（内核 <code>allow_hot_geom=N</code>，不在线热改几何）。若配置了 <code>kernel_vtl_reload_script</code> 或 ioctl 可用则自动对齐。<strong><code>kernel_reload_on_db_change</code> 默认为 <code>false</code></strong>。勾选下方可在保存后立即 ioctl 对齐；整模块 <code>rmmod</code> 仅当显式开启 <code>kernel_reload_on_db_change</code> 且 ioctl 失败时才会尝试，并在磁带设备仍被占用时拒绝。详见 <code>docs/SCSI.md</code> §1c。</p>
<div id="done" class="setup-done setup-hint">主配置已存在。<a href="/admin/overview">进入后台总览</a></div>
<form id="sf" style="display:none">
<label>db_path（SQLite 文件）</label><input name="db_path" type="text" autocomplete="off"/>
<label>tape_dir（磁带镜像目录）</label><input name="tape_dir" type="text" autocomplete="off"/>
<label>log_dir（运行日志目录）</label><input name="log_dir" type="text" autocomplete="off"/>
<label>kernel_vtl_reload_script（可选）</label><input name="kernel_vtl_reload_script" type="text" autocomplete="off"/>
<label>vtl_ko（可选，写入 vtl.conf；vtladm 调用重载脚本时注入环境变量 VTL_KO）</label><input name="vtl_ko" type="text" autocomplete="off"/>
<label>vtl_reload_scan_delay_ms（可选，毫秒；写入 vtl.conf；重载脚本收到 VTL_SCAN_DELAY_MS，对应 insmod 的 scan_delay_ms）</label><input name="vtl_reload_scan_delay_ms" type="text" inputmode="numeric" autocomplete="off"/>
<label style="margin-top:.65rem"><input name="run_kernel_reload_now" type="checkbox" value="1"/> 提交成功后<strong>立即</strong>尝试对齐内核几何（默认仅 <code>/dev/vtl</code> ioctl，<strong>不</strong>跑 <code>rmmod</code>；仅当 <code>kernel_reload_on_db_change=true</code> 且 ioctl 失败时才可能执行 <code>kernel_vtl_reload_script</code>，有磁带占用时会拒绝 <code>rmmod</code>）</label>
<button type="submit">保存并创建配置</button>
<p class="err" id="se"></p>
</form>
</div>
<script>
async function boot(){
  const r=await fetch('/api/setup/status',{credentials:'include'});
  const j=await r.json().catch(()=>({}));
  if(!j.setup_required){
    document.getElementById('done').style.display='block';
    return;
  }
  const d=j.defaults||{};
  const f=document.getElementById('sf');
  f.style.display='block';
  f.querySelector('[name=db_path]').placeholder=d.db_path||'';
  f.querySelector('[name=tape_dir]').placeholder=d.tape_dir||'';
  f.querySelector('[name=log_dir]').placeholder=d.log_dir||'';
  f.querySelector('[name=kernel_vtl_reload_script]').placeholder=d.kernel_vtl_reload_script||'/opt/vtladm/scripts/vtl-kernel-reload.sh';
  f.querySelector('[name=vtl_ko]').placeholder=d.vtl_ko||'/opt/vtladm/ko/vtl.ko';
  f.querySelector('[name=vtl_reload_scan_delay_ms]').placeholder='留空则脚本默认 500；仍不稳可 vtl.conf 写大或 export VTL_POST_ADD_SCAN_DELAY_MS=1000';
}
boot();
document.getElementById('sf').onsubmit=async(ev)=>{
  ev.preventDefault();
  document.getElementById('se').textContent='';
  const fd=new FormData(ev.target);
  const body={
    db_path:(fd.get('db_path')||'').toString().trim(),
    tape_dir:(fd.get('tape_dir')||'').toString().trim(),
    log_dir:(fd.get('log_dir')||'').toString().trim(),
    kernel_vtl_reload_script:(fd.get('kernel_vtl_reload_script')||'').toString().trim(),
    vtl_ko:(fd.get('vtl_ko')||'').toString().trim(),
    vtl_reload_scan_delay_ms:(fd.get('vtl_reload_scan_delay_ms')||'').toString().trim(),
    run_kernel_reload_now:!!fd.get('run_kernel_reload_now')
  };
  const r=await fetch('/api/setup/complete',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body),credentials:'include'});
  const j=await r.json().catch(()=>({}));
  if(r.ok){location.href='/admin/overview';return;}
  document.getElementById('se').textContent=j.error||('HTTP '+r.status);
};
</script>
</body></html>
"#
);

// 登录页：**不使用 <form>**。
//
// 历史 bug：旧版用 <form id="f"> 但未设 action，靠 onsubmit 里的 ev.preventDefault()
// 阻止默认提交。一旦 JS 因任何原因（绑定时机、autofill 提前提交、双击、CSP）
// 没拦截住，浏览器就把表单按默认行为 POST 到当前 URL `/login`——而 `/login`
// 是 GET 路由，结果是渲染同一个登录页，用户看到的就是"只刷新就结束"。
//
// 重写方案：完全用 <div> 容器 + <button type="button">，物理上消除"默认表单提交"
// 这条故障路径。所有交互都走显式 addEventListener；fetch 失败、网络异常、JSON 解析
// 失败均有可见错误提示，不会静默回退。
pub(super) const LOGIN_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width,initial-scale=1"/>
<meta name="referrer" content="no-referrer"/>
<title>VTL 登录</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
body.login-page{display:flex;min-height:100vh;align-items:center;justify-content:center;padding:1.25rem;margin:0;}
.login-card{width:100%;max-width:420px;background:var(--card);border:1px solid var(--border);border-radius:10px;padding:1.25rem 1.35rem;box-shadow:0 4px 20px rgba(15,23,42,.08);}
.login-card h1{margin:0 0 .5rem;font-size:1.2rem;color:var(--accent);}
.login-hint{font-size:.84rem;color:var(--muted);line-height:1.55;margin:0 0 1rem;}
.login-hint code{font-size:.8rem;background:#f1f5f9;padding:.1rem .35rem;border-radius:4px;border:1px solid #e2e8f0;}
.login-card label{display:block;margin:.6rem 0 .2rem;font-size:.92rem;}
.login-card input{width:100%;box-sizing:border-box;padding:.45rem .55rem;font-size:.95rem;}
.login-card .captcha-row{display:flex;align-items:center;gap:.6rem;}
.login-card .captcha-q{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:1rem;letter-spacing:.05em;}
.login-card .btn-link{background:none;border:none;color:var(--accent);cursor:pointer;font-size:.84rem;padding:0;text-decoration:underline;}
.login-card .btn-link:disabled{color:var(--muted);cursor:wait;text-decoration:none;}
.login-card #submitBtn{margin-top:1.1rem;width:100%;padding:.6rem;font-weight:600;font-size:.98rem;cursor:pointer;}
.login-card #submitBtn:disabled{opacity:.65;cursor:wait;}
.login-card #msg{margin:.7rem 0 0;font-size:.88rem;min-height:1.2em;line-height:1.45;word-break:break-word;}
.login-card #msg.err{color:#b91c1c;}
.login-card #msg.ok{color:#047857;}
.login-card .login-foot{margin:.85rem 0 0;font-size:.84rem;color:var(--muted);}
.login-card .login-foot a{color:var(--accent);}
</style>
</head>
<body class="login-page">
<div class="login-card" id="loginCard">
  <h1>VTL Web 登录</h1>
  <p class="login-hint">默认用户 <code>admin</code>。首次运行会在日志目录生成 <code>web_admin.json</code>（bcrypt 哈希）；初始密码请从安装输出或服务器端 <code>vtladm reset-web-auth</code> 获取，登录后请尽快修改。</p>

  <label for="u">用户名</label>
  <input id="u" type="text" autocomplete="username" autocapitalize="none" spellcheck="false" required/>

  <label for="p">密码</label>
  <input id="p" type="password" autocomplete="current-password" required/>

  <label for="a">验证码</label>
  <div class="captcha-row">
    <span id="q" class="captcha-q">加载中…</span>
    <button id="captchaReload" type="button" class="btn-link" title="换一题">换一题</button>
  </div>
  <input id="a" type="text" inputmode="numeric" autocomplete="off" spellcheck="false" required/>

  <button id="submitBtn" type="button">登录</button>

  <p id="msg" role="status" aria-live="polite"></p>
  <p class="login-foot"><a href="/">返回首页</a>（须先登录后访问）。</p>
</div>

<script>
(function () {
  "use strict";

  var state = { captchaId: "", busy: false };

  function $(id) { return document.getElementById(id); }

  function setMsg(text, kind) {
    var el = $("msg");
    if (!el) return;
    el.textContent = text || "";
    el.className = kind || "";
  }

  function setBusy(b) {
    state.busy = b;
    var btn = $("submitBtn");
    if (btn) {
      btn.disabled = b;
      btn.textContent = b ? "登录中…" : "登录";
    }
    var rl = $("captchaReload");
    if (rl) rl.disabled = b;
  }

  function getInput(id) {
    var el = $(id);
    return el ? String(el.value || "") : "";
  }

  async function loadCaptcha() {
    var q = $("q");
    var rl = $("captchaReload");
    if (q) q.textContent = "加载中…";
    if (rl) rl.disabled = true;
    state.captchaId = "";
    try {
      var r = await fetch("/api/captcha", { credentials: "include", cache: "no-store" });
      if (!r.ok) throw new Error("HTTP " + r.status);
      var j = await r.json();
      state.captchaId = j.captcha_id || "";
      if (q) q.textContent = (j.question || "(空)").trim();
      var ans = $("a");
      if (ans) ans.value = "";
    } catch (e) {
      if (q) q.textContent = "加载失败";
      setMsg("无法获取验证码：" + (e && e.message ? e.message : e) + "（请检查与服务器的连接，然后点「换一题」）", "err");
    } finally {
      if (rl && !state.busy) rl.disabled = false;
    }
  }

  // 登录成功后调用：检测会话 cookie 是否被浏览器接受。
  // 返回 { ok:true, target } 或 { ok:false, reason }。
  async function probeSessionAndPickTarget() {
    var s;
    try {
      s = await fetch("/api/setup/status", { credentials: "include", cache: "no-store" });
    } catch (e) {
      return { ok: false, reason: "网络错误：" + (e && e.message ? e.message : e) };
    }
    if (s.status === 401 || s.status === 403) {
      // 服务端登录成功（已 Set-Cookie），但下一请求未携带 cookie——典型原因：
      //  - 服务端 cookie 带 Secure 但当前用 HTTP（浏览器静默丢弃）
      //  - 浏览器禁用了 cookie / 第三方 cookie 拦截
      //  - 反代未透传 Set-Cookie / Cookie
      return {
        ok: false,
        reason: "登录成功但浏览器未保留会话 cookie。常见原因：\n"
          + "  1) 服务端启用了 Cookie Secure 但你用 HTTP 访问 → 在服务器 unset VTLADM_WEB_COOKIE_SECURE 或设为 0 后重启 vtladm-web；\n"
          + "  2) 浏览器禁用了 cookie / 隐私模式拦截 → 允许此站点 cookie；\n"
          + "  3) 反代未透传 Set-Cookie / Cookie 头 → 检查 nginx/traefik 配置。"
      };
    }
    if (!s.ok) {
      // 其他 5xx 不阻塞跳转
      return { ok: true, target: "/admin/library" };
    }
    var sj = {};
    try { sj = await s.json(); } catch (_) {}
    var target = (sj && sj.setup_required) ? "/admin/setup-init" : "/admin/library";
    return { ok: true, target: target };
  }

  async function doLogin() {
    if (state.busy) return;
    setMsg("", "");
    var u = getInput("u").trim();
    var p = getInput("p");          // 密码不 trim：保留前后空格的合法字符
    var a = getInput("a").trim();
    if (!u || !p || !a) { setMsg("请填写用户名、密码和验证码", "err"); return; }
    if (!state.captchaId) { setMsg("验证码未加载完成，请稍候或点「换一题」", "err"); return; }

    setBusy(true);
    try {
      var resp;
      try {
        resp = await fetch("/api/login", {
          method: "POST",
          headers: { "Content-Type": "application/json", "Accept": "application/json" },
          body: JSON.stringify({
            username: u,
            password: p,
            captcha_id: state.captchaId,
            captcha_answer: a
          }),
          credentials: "include",
          cache: "no-store"
        });
      } catch (netErr) {
        setMsg("网络错误：无法连接服务器（" + (netErr && netErr.message ? netErr.message : netErr) + "）", "err");
        return;
      }

      var body = {};
      try { body = await resp.json(); } catch (_) { /* 非 JSON 响应：保持 body={} */ }

      if (resp.ok && body && body.ok) {
        if (body.must_change_password) {
          setMsg("首次登录，请先修改默认密码", "ok");
          window.location.assign("/admin/account");
          return;
        }
        setMsg("登录成功，校验会话…", "ok");
        var probe = await probeSessionAndPickTarget();
        if (!probe.ok) {
          setMsg(probe.reason, "err");
          await loadCaptcha();
          return;
        }
        setMsg("会话已建立，跳转中…", "ok");
        window.location.assign(probe.target);
        return;
      }

      var msg = (body && body.error) ? body.error : ("登录失败（HTTP " + resp.status + "）");
      if (body && body.hint) msg += " — " + body.hint;
      if (body && typeof body.retry_after_secs === "number") {
        msg += "（约 " + body.retry_after_secs + " 秒后可重试）";
      }
      setMsg(msg, "err");
      // 验证码单次消费，无论成败都换一题
      await loadCaptcha();
    } finally {
      setBusy(false);
    }
  }

  function onKey(ev) {
    if (ev.key === "Enter" || ev.keyCode === 13) {
      ev.preventDefault();
      doLogin();
    }
  }

  function init() {
    var btn = $("submitBtn");
    if (btn) btn.addEventListener("click", function (ev) { ev.preventDefault(); doLogin(); });
    var rl = $("captchaReload");
    if (rl) rl.addEventListener("click", function (ev) { ev.preventDefault(); loadCaptcha(); });
    ["u", "p", "a"].forEach(function (id) {
      var el = $(id);
      if (el) el.addEventListener("keydown", onKey);
    });
    var first = $("u");
    if (first) { try { first.focus(); } catch (_) {} }
    loadCaptcha();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
</script>
</body>
</html>
"#
);

pub(super) const ADMIN_OVERVIEW_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — 后台概览</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
.grid-minis{display:grid;gap:.65rem;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));}
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_admin_side_inner.html"),
    r#"</aside>
<main class="adm-main adm-workspace">
<div class="breadcrumb">存储 · 虚拟磁带库 <b>›</b> <b>概览</b></div>
<div class="topbar">
<div><h1 style="margin:0;font-size:1.28rem;">后台概览</h1>
<p style="margin:.35rem 0 0;color:var(--muted);font-size:.9rem;max-width:44rem;">按 <strong>① 建库 → ② 磁带/槽位 → ③ 传输 → ④ 备份软件</strong> 使用；详见 <a href="/admin/transport">传输向导</a> 与 <code>docs/WEB-WORKFLOW.md</code>。</p></div>
<div><span class="badge-w">已登录</span> <button type="button" id="btn-logout">登出</button></div>
</div>
<div class="grid-minis">
<a class="mini" href="/admin/library"><strong>① 磁带库</strong><span>建库、几何、删库</span></a>
<a class="mini" href="/admin/tapes"><strong>② 磁带与货架</strong><span>批量建带、迁移、删带</span></a>
<a class="mini" href="/admin/assign-slot"><strong>② 磁带入槽</strong><span>货架 → 在线库槽位</span></a>
<a class="mini" href="/admin/changer"><strong>② inventory 对账</strong><span>备份搬带后 DB↔内核</span></a>
<a class="mini" href="/admin/transport"><strong>③ 传输向导</strong><span>SCSI / iSCSI / FC</span></a>
<a class="mini" href="/admin/iscsi"><strong>③ iSCSI 映射</strong><span>library-export，记录入库</span></a>
<a class="mini" href="/admin/shelf"><strong>货架</strong><span>在线/离线货架</span></a>
<a class="mini" href="/admin/shelf-place"><strong>磁带出库</strong><span>在线库 → 离线</span></a>
<a class="mini" href="/admin/account"><strong>账户与安全</strong><span>修改登录密码</span></a>
</div>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"document.getElementById('btn-logout').onclick=async()=>{
  await fetch('/api/logout',{method:'POST',credentials:'include'});
  location.href='/login';
};
</script></body></html>
"#
);

pub(super) const ADMIN_TRANSPORT_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — 传输向导</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
.workflow{display:grid;gap:.75rem;grid-template-columns:repeat(auto-fill,minmax(280px,1fr));margin:1rem 0;}
.wstep{border:1px solid var(--border,#ddd);border-radius:8px;padding:.85rem 1rem;background:var(--panel,#fafafa);}
.wstep h3{margin:0 0 .35rem;font-size:1rem;}
.wstep p{margin:0;font-size:.88rem;color:var(--muted,#555);line-height:1.45;}
.wstep a{display:inline-block;margin-top:.5rem;font-size:.88rem;}
.wstep-num{color:var(--accent,#2980b9);font-weight:600;}
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_admin_side_inner.html"),
    r#"</aside>
<main class="adm-main adm-workspace">
<div class="breadcrumb">存储功能 <b>›</b> 虚拟磁带库 <b>›</b> <b>传输向导</b></div>
<div class="topbar" style="justify-content:space-between;flex-wrap:wrap;gap:.75rem">
<div><h1 style="margin:0;font-size:1.22rem;">传输链路（SCSI / iSCSI / FC）</h1>
<p class="hint" style="margin:.35rem 0 0;max-width:44rem">本页汇总<strong>③ 传输层</strong>：内核在本机提供 SCSI 带库语义；可选 iSCSI（Web 一键 export）或 FC（系统级 target）供<strong>备份软件</strong>作为存储层连接。完整分层见仓库 <code>userspace/docs/WEB-WORKFLOW.md</code>。</p></div>
<button type="button" id="btn-logout">登出</button>
</div>
<section class="panel">
<h2 style="margin:0 0 .6rem;font-size:1rem">推荐顺序</h2>
<div class="workflow">
<div class="wstep"><h3><span class="wstep-num">①</span> 磁带库</h3><p>先建库并对齐 <code>vtl.ko</code> 几何（驱动器数、槽位数）。</p><a href="/admin/library">建库与库属性 →</a></div>
<div class="wstep"><h3><span class="wstep-num">②</span> 磁带与槽位</h3><p>创建 <code>.vtltape</code> 镜像，必要时入槽；DB 记录槽位/货架。</p><a href="/admin/tapes">磁带与货架 →</a></div>
<div class="wstep"><h3><span class="wstep-num">③</span> 传输（本页）</h3><p>按部署选择 local / iSCSI / FC，使备份机可见 SCSI 带库。</p></div>
<div class="wstep"><h3><span class="wstep-num">④</span> 备份软件</h3><p>在备份侧添加存储单元、扫描带库；换带与备份任务在备份侧完成（非 Web）。</p></div>
</div>
</section>
<section class="panel">
<h2 style="margin:0 0 .6rem;font-size:1rem">三种承载方式</h2>
<table class="data-table"><thead><tr><th>方式</th><th>适用</th><th>Web / 工具</th></tr></thead><tbody>
<tr><td><strong>SCSI（local）</strong></td><td>备份软件与本机同台</td><td>下方按库扫描 <code>/dev/sg*</code>、<code>/dev/st*</code>（仅显示建库时配置的驱动器数）</td></tr>
<tr><td><strong>iSCSI</strong></td><td>备份机经以太网</td><td>先扫描核对节点，再在 <a href="/admin/iscsi">iSCSI / LUN 映射</a> 执行 library-export</td></tr>
<tr><td><strong>FC</strong></td><td>SAN / 光纤</td><td>下方扫描本机应对节点；FC target 由系统配置，见 <code>docs/TRANSPORT.md</code></td></tr>
</tbody></table>
<p class="hint" id="transport-limits-hint" style="margin-top:.75rem">产品上限：在线库最多 <strong>8</strong> 个；每库最多 <strong>8</strong> 台驱动器、<strong>256</strong> 个数据槽。内核可能枚举更多磁带 LUN，界面与导出<strong>仅使用前 N 台</strong>（N = 建库驱动数）。</p>
<p class="hint" style="margin-top:.5rem">SCSI 是磁带/机械手<strong>设备模型</strong>；iSCSI 与 FC 是在网络上<strong>承载同一套 SCSI</strong>，并非三套独立产品功能。</p>
</section>
<section class="panel">
<h2 style="margin:0 0 .6rem;font-size:1rem">按库核对 SCSI 设备（local / iSCSI / FC 共用）</h2>
<p class="hint">选择在线库后扫描；结果行数 = 1 机械手 + 库内磁带机数（与「磁带库」页建库时填写一致）。</p>
<label>当前在线库</label><select id="tselib" style="max-width:16rem"></select>
<label style="margin-left:.75rem">承载方式（仅影响说明文案）</label>
<select id="tmode"><option value="local">SCSI（本机）</option><option value="iscsi">iSCSI</option><option value="fc">FC</option></select>
<button type="button" id="btn-scan-dev">扫描 lsscsi（VTL）</button>
<p class="err" id="tscan-err"></p>
<p class="hint" id="tscan-note"></p>
<table class="data-table" style="margin-top:.5rem"><thead><tr><th>角色</th><th>LUN</th><th>/dev/sg</th><th>/dev/st 或 sch</th></tr></thead><tbody id="tscan-body"><tr><td colspan="4" class="muted">尚未扫描</td></tr></tbody></table>
<pre id="tscan-raw" style="max-height:8rem;overflow:auto;font-size:.78rem;margin-top:.5rem"></pre>
</section>
<section class="panel">
<h2 style="margin:0 0 .6rem;font-size:1rem">当前配置</h2>
<button type="button" id="btn-fabric">刷新</button>
<pre id="fabric-out" style="max-height:14rem;overflow:auto;margin-top:.5rem"></pre>
<p class="hint">CLI：<code>vtladm transport show|check|guide</code> · 定时：<code>vtl-patrol.timer</code></p>
<p><button type="button" id="btn-patrol">运行巡检</button></p>
<pre id="patrol-out" style="max-height:14rem;overflow:auto;margin-top:.5rem"></pre>
</section>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"let VTL_LIMITS={max_online_libraries:8,max_drives_per_library:8,max_data_slots_per_library:256};
function applyProductLimitsFromApi(j){
  if(!j||!j.product_limits)return;
  const L=j.product_limits;
  VTL_LIMITS={max_online_libraries:Number(L.max_online_libraries)||8,max_drives_per_library:Number(L.max_drives_per_library)||8,max_data_slots_per_library:Number(L.max_data_slots_per_library)||256};
  const h=document.getElementById('transport-limits-hint');
  if(h){h.innerHTML='产品上限：在线库最多 <strong>'+VTL_LIMITS.max_online_libraries+'</strong> 个；每库最多 <strong>'+VTL_LIMITS.max_drives_per_library+'</strong> 台驱动器、<strong>'+VTL_LIMITS.max_data_slots_per_library+'</strong> 个数据槽。内核可能枚举更多磁带 LUN，界面与导出<strong>仅使用前 N 台</strong>（N = 建库驱动数）。';}
}
function escapeHtml(s){return String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
function onlineLibs(j){return (j.libraries||[]).filter(l=>{const n=l.name||'';if(n==='__offline__')return false;if(l.is_offline_storage)return false;return !!n;});}
async function loadTransportLibs(){
  const sel=document.getElementById('tselib');
  const r=await fetch('/api/libraries',{credentials:'include'});
  const j=await r.json();
  if(!r.ok)return;
  applyProductLimitsFromApi(j);
  sel.innerHTML='';
  onlineLibs(j).forEach(l=>{const o=document.createElement('option');o.value=l.name;o.textContent=l.name;sel.appendChild(o);});
  const q=new URLSearchParams(location.search).get('library');
  if(q&&[...sel.options].some(o=>o.value===q))sel.value=q;
}
function renderScanDevices(j){
  const tb=document.getElementById('tscan-body');
  const devs=j.devices||[];
  if(!devs.length){tb.innerHTML='<tr><td colspan="4" class="muted">无设备</td></tr>';return;}
  tb.innerHTML=devs.map(d=>{
    const role=d.role==='changer'?'机械手':'磁带机 '+(d.index!=null?d.index:'');
    const aux=d.role==='changer'?(d.sch||'—'):(d.st||'—');
    return '<tr><td>'+escapeHtml(role)+'</td><td class="num">'+(d.lun!=null?d.lun:'')+'</td><td><code>'+escapeHtml(d.sg||'—')+'</code></td><td><code>'+escapeHtml(aux)+'</code></td></tr>';
  }).join('');
}
document.getElementById('btn-logout').onclick=async()=>{await fetch('/api/logout',{method:'POST',credentials:'include'});location.href='/login';};
document.getElementById('btn-scan-dev').onclick=async()=>{
  document.getElementById('tscan-err').textContent='';
  const lib=document.getElementById('tselib').value;
  const mode=document.getElementById('tmode').value;
  if(!lib){document.getElementById('tscan-err').textContent='请选择在线库';return;}
  const url='/api/manage/transport/scan-sg?library='+encodeURIComponent(lib)+'&transport='+encodeURIComponent(mode);
  const r=await fetch(url,{credentials:'include'});
  const j=await r.json();
  if(!r.ok){document.getElementById('tscan-err').textContent=j.error||r.status;document.getElementById('tscan-note').textContent='';return;}
  document.getElementById('tscan-note').textContent=(j.note||'')+' SCSI host '+j.picked_scsi_host+'，驱动器 '+j.drive_count+' 台。';
  renderScanDevices(j);
  document.getElementById('tscan-raw').textContent=j.raw_tail||'';
};
async function loadFabric(){
  const r=await fetch('/api/fabric',{credentials:'include'});
  const j=await r.json();
  applyProductLimitsFromApi(j);
  document.getElementById('fabric-out').textContent=JSON.stringify(j,null,2);
  const mode=(j.transport||'local').toLowerCase();
  const sel=document.getElementById('tmode');
  if(sel&&[...sel.options].some(o=>o.value===mode))sel.value=mode;
}
document.getElementById('btn-fabric').onclick=loadFabric;
document.getElementById('btn-patrol').onclick=async()=>{
  const el=document.getElementById('patrol-out');
  el.textContent='…';
  const r=await fetch('/api/patrol',{credentials:'include'});
  const j=await r.json();
  el.textContent=(j.stdout||'')+(j.stderr?'\n'+j.stderr:'')+'\nexit_code='+j.exit_code;
};
(async()=>{await loadTransportLibs();await loadFabric();})();
</script></body></html>
"#
);

pub(super) const ADMIN_ACCOUNT_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — 账户与安全</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
main.adm-main{max-width:36rem;}
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_admin_side_inner.html"),
    r#"</aside>
<main class="adm-main adm-workspace">
<div class="breadcrumb">存储 · 虚拟磁带库 <b>›</b> <b>系统</b> <b>›</b> <b>账户与安全</b></div>
<div class="topbar"><h1 style="margin:0;font-size:1.2rem;">账户与安全</h1><button type="button" id="btn-logout">登出</button></div>
<section class="panel"><h2>修改密码</h2>
<label>原密码</label><input id="op" type="password"/>
<label>新密码（至少 8 字符）</label><input id="np" type="password"/>
<button type="button" id="chgpw">保存</button><p class="err" id="pe"></p>
</section>
t<section class="panel"><h2>活跃会话</h2>
	<p class="hint">当前所有活跃登录会话。撤销将强制该会话登出。</p>
	<div id="sessions"></div>
	<button type="button" id="load-sessions">刷新会话列表</button>
	</section>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"function escapeHtml(s){return String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'})[c]);}
async function jpost(url,body){
  const r=await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},credentials:'include',body:JSON.stringify(body)});
  const t=await r.text();let j;try{j=JSON.parse(t);}catch{j={raw:t};}return{r,j};
}
document.getElementById('btn-logout').onclick=async()=>{await fetch('/api/logout',{method:'POST',credentials:'include'});location.href='/login';};
document.getElementById('chgpw').onclick=async()=>{
  document.getElementById('pe').textContent='';
  const {r,j}=await jpost('/api/change-password',{old_password:document.getElementById('op').value,new_password:document.getElementById('np').value});
  if(!r.ok){document.getElementById('pe').textContent=j.error||r.status;return;}
  showToast('密码已修改，请重新登录');
  setTimeout(function(){location.href='/login';},1500);
};
async function loadSessions(){
  const r=await fetch('/api/sessions',{credentials:'include'});
  const j=await r.json();
  const el=document.getElementById('sessions');
  if(!j.sessions||!j.sessions.length){el.innerHTML='<p class="hint">无其他活跃会话</p>';return;}
  let h='<table><thead><tr><th>会话</th><th>用户</th><th>创建时间</th><th>操作</th></tr></thead><tbody>';
  j.sessions.forEach(function(s){
    h+='<tr><td>'+escapeHtml(s.token_prefix)+'</td><td>'+escapeHtml(s.username)+'</td><td>'+s.created_secs_ago+'s 前</td>';
    if(s.is_current){h+='<td><span class="badge-w">当前</span></td>';}
    else{h+='<td><button type="button" data-token="'+escapeHtml(s.token_prefix)+'" class="revoke-session">撤销</button></td>';}
    h+='</tr>';
  });
  h+='</tbody></table>';
  el.innerHTML=h;
  el.querySelectorAll('.revoke-session').forEach(function(btn){
    btn.onclick=async function(){
      const t=this.getAttribute('data-token');
      const rr=await jpost('/api/sessions/revoke',{token:t});
      if(!rr.r.ok){showToast(rr.j.error||'撤销失败');return;}
      showToast('已撤销会话 '+t);
      loadSessions();
    };
  });
}
document.getElementById('load-sessions').onclick=loadSessions;
loadSessions();
</script></body></html>
"#
);

pub(super) const ADMIN_TAPES_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — 磁带与货架</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
input,select{max-width:36rem;}
.btn-del{color:#c0392b!important;font-weight:600;}
.btn-del:hover{background:#c0392b;color:#fff!important;}
.btn-init{color:#2980b9!important;}
.loading-mask{pointer-events:none;opacity:.5;}
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_admin_side_inner.html"),
    r#"</aside>
<main class="adm-main adm-workspace">
<div class="breadcrumb">存储 · 虚拟磁带库 <b>›</b> <b>磁带与货架</b></div>
<div class="topbar"><h1 style="margin:0;font-size:1.2rem;">磁带与货架</h1><div><span class="badge-w">已登录</span> <button type="button" id="btn-logout">登出</button></div></div>
<p class="hint" style="margin-top:0">先选择<strong>当前在线库</strong>；下方按步骤切换标签，避免创建、迁移与删除混在同一屏。</p>
<div class="ctx-strip">
<div class="ctx-strip-inner">
<label class="ctx-label" for="tlib">当前在线库</label>
<select id="tlib"></select>
</div>
</div>
<nav class="panel-tabs" role="tablist" aria-label="磁带操作">
<button type="button" class="ptab active" role="tab" id="tabbtn-create" data-tab="create" aria-selected="true">批量创建</button>
<button type="button" class="ptab" role="tab" id="tabbtn-migrate" data-tab="migrate" aria-selected="false">货架迁移</button>
<button type="button" class="ptab" role="tab" id="tabbtn-maintain" data-tab="maintain" aria-selected="false">初始化 / 删除</button>
</nav>
<div id="tab-create" class="tab-panel active" role="tabpanel" aria-labelledby="tabbtn-create">
<section class="panel"><h2>批量创建磁带</h2>
<p class="hint">名称由程序按 <code>{库名}_tape</code> + 数字自动生成（如 <code>marstor_tape01</code>）；<strong>磁带名在所有库间须全局唯一</strong>（内核扁平 <code>tape_dir</code>）。条码自动随机。<code>robot sync</code> 前会自动链接镜像；链接失败则 sync 中止。只需选<strong>货架</strong>、<strong>数量</strong>、<strong>容量</strong>。CLI/API 手动建带亦须遵守全局唯一命名。</p>
<label>货架</label><select id="tshelf"><option value="">默认货架</option></select>
<label>数量（1–10000）</label><input type="number" id="tcnt" min="1" max="10000" value="10"/>
<label>容量（如 500M、2G）</label><input id="tsize" placeholder="500M"/>
<label>密度格式</label><select id="tdensity">
<option value="0x40">Default LTO</option>
<option value="0x4A">LTO-5</option>
<option value="0x4C">LTO-6</option>
<option value="0x4E">LTO-7</option>
<option value="0x50">LTO-8</option>
<option value="0x52">LTO-9</option>
<option value="0x58">LTO-10</option>
</select>
<button type="button" id="btauto">创建磁带</button><p class="err" id="te"></p>
</section>
</div>
<div id="tab-migrate" class="tab-panel" role="tabpanel" aria-labelledby="tabbtn-migrate">
<section class="panel"><h2>货架间批量迁移</h2>
<p class="hint">在同一在线库内，将磁带从<strong>源货架</strong>迁到<strong>目标货架</strong>。仅列出在源货架上、<strong>未入槽</strong>且<strong>未在驱动</strong>的磁带；不移动镜像文件。</p>
<label>源货架</label><select id="mfrom"><option value="">请选择源货架</option></select>
<label>目标货架</label><select id="mto"><option value="">请选择目标货架</option></select>
<div class="row-actions"><button type="button" id="mst">全选</button><button type="button" id="mclr">清除勾选</button></div>
<div id="mwrap"></div>
<button type="button" id="bmig">批量迁移</button><p class="err" id="me"></p>
</section>
</div>
<div id="tab-maintain" class="tab-panel" role="tabpanel" aria-labelledby="tabbtn-maintain">
<section class="panel"><h2>初始化与删除磁带</h2>
<p class="hint">初始化：将 <code>used_bytes</code> 置 0，并把镜像文件截断为标称容量（空白带）。磁带须<strong>在货架上</strong>（未入机械手槽）、<strong>不在驱动中</strong>。删除会移除数据库记录并删除镜像文件；若删除镜像失败会在服务端记错误日志，界面仍可能显示成功（需检查磁盘）。</p>
<div class="row-actions"><button type="button" id="treload">刷新列表</button></div>
<div id="tmwrap"></div><p class="err" id="tme"></p>
</section>
</div>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"function fmtBytes(n){n=Number(n)||0;if(n===0)return'0 B';const u=['B','KB','MB','GB','TB'];let i=0,x=n;while(x>=1024&&i<u.length-1){x/=1024;i++;}return (x>=100||i===0?x.toFixed(0):x.toFixed(1))+' '+u[i];}
function escapeHtml(s){return String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
async function jpost(url,body){
  const r=await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},credentials:'include',body:JSON.stringify(body)});
  const t=await r.text();let j;try{j=JSON.parse(t);}catch{j={raw:t};}return{r,j};
}
function onlineLibs(j){return (j.libraries||[]).filter(l=>!l.is_offline_storage&&l.name);}
document.getElementById('btn-logout').onclick=async()=>{await fetch('/api/logout',{method:'POST',credentials:'include'});location.href='/login';};
function initTapeTabs(){
  document.querySelectorAll('.ptab').forEach(btn=>{
    btn.addEventListener('click',()=>{
      const id=btn.getAttribute('data-tab');
      document.querySelectorAll('.ptab').forEach(b=>{
        const on=b===btn;
        b.classList.toggle('active',on);
        b.setAttribute('aria-selected',on?'true':'false');
      });
      document.querySelectorAll('.tab-panel').forEach(p=>{
        p.classList.toggle('active',p.id==='tab-'+id);
      });
    });
  });
}
initTapeTabs();
function appendShelfOptions(sel, shelves, withDefault){
  if(withDefault){
    const o=document.createElement('option');o.value='';o.textContent='默认货架';sel.appendChild(o);
  }
  (shelves||[]).forEach(s=>{
    const o=document.createElement('option');o.value=s.name;
    const tag=s.is_default_unused?'（未使用默认）':'';
    o.textContent=s.name+tag;sel.appendChild(o);
  });
}
async function loadLibs(){
  document.getElementById('te').textContent='';
  const r=await fetch('/api/libraries',{credentials:'include'});
  const j=await r.json();
  const sel=document.getElementById('tlib');
  sel.innerHTML='';
  if(!r.ok){document.getElementById('te').textContent=j.error||r.status;return;}
  onlineLibs(j).forEach(l=>{const o=document.createElement('option');o.value=l.name;o.textContent=l.name+' (#'+l.id+')';sel.appendChild(o);});
  const qt=new URLSearchParams(location.search).get('library');
  if(qt&&[...sel.options].some(o=>o.value===qt)) sel.value=qt;
  sel.onchange=async()=>{await loadShelves();await loadMigrateTable();await loadTapeMaintainTable();};
  if(sel.options.length) await loadShelves();
  if(sel.options.length) await loadMigrateTable();
  if(sel.options.length) await loadTapeMaintainTable();
  const tab=new URLSearchParams(location.search).get('tab');
  if(tab==='maintain'){const b=document.getElementById('tabbtn-maintain');if(b)b.click();}
  else if(tab==='migrate'){const b=document.getElementById('tabbtn-migrate');if(b)b.click();}
  else if(tab==='create'){const b=document.getElementById('tabbtn-create');if(b)b.click();}
}
async function loadTapeMaintainTable(){
  const lib=document.getElementById('tlib').value;
  const w=document.getElementById('tmwrap');
  const errEl=document.getElementById('tme');
  errEl.textContent='';
  w.innerHTML='';
  if(!lib)return;
  const tr=await fetch('/api/tapes?library='+encodeURIComponent(lib),{credentials:'include'});
  const tj=await tr.json();
  if(!tr.ok){errEl.textContent=tj.error||tr.status;return;}
  const rows=tj.tapes||[];
  if(!rows.length){w.innerHTML='<div class="empty">当前库无磁带</div>';return;}
  let h='<table class="data-table"><thead><tr><th>名称</th><th>条码</th><th class="num">容量</th><th class="num">已用</th><th>位置</th><th class="ops">操作</th></tr></thead><tbody>';
  rows.forEach(t=>{
    const loc=t.in_drive?'驱动中':(t.slot!=null?('槽位 '+t.slot):('架: '+(t.shelf_name?escapeHtml(t.shelf_name):'—')));
    const enc=encodeURIComponent(t.name);
    h+='<tr><td>'+escapeHtml(t.name)+'</td><td>'+escapeHtml(t.barcode)+'</td><td class="num">'+fmtBytes(t.capacity_bytes)+'</td><td class="num">'+fmtBytes(t.used_bytes||0)+'</td><td>'+loc+'</td><td class="ops"><button type="button" class="lnk btn-init" data-name="'+enc+'">初始化</button> <button type="button" class="lnk btn-del" data-name="'+enc+'" title="不可恢复！">⛔ 删除</button></td></tr>';
  });
  if(tj.truncated){h+='<tr><td colspan="6" style="text-align:center;color:var(--muted)">（已显示前 '+rows.length+' 条，共 '+tj.total+' 条；如需查看更多请使用 CLI）</td></tr>';}
  h+='</tbody></table>';w.innerHTML=h;
}
document.getElementById('treload').onclick=loadTapeMaintainTable;
document.getElementById('tmwrap').addEventListener('click',async (ev)=>{
  const t=ev.target;
  if(!t.classList.contains('btn-init')&&!t.classList.contains('btn-del'))return;
  const lib=document.getElementById('tlib').value;
  const name=decodeURIComponent(t.getAttribute('data-name'));
  const errEl=document.getElementById('tme');
  errEl.textContent='';
  if(t.classList.contains('btn-del')){
    if(!confirm('⚠️ 确定永久删除磁带「'+name+'」？\n此操作不可恢复，磁带数据将被清除。'))return;
    t.disabled=true;t.textContent='删除中…';
    const {r,j}=await jpost('/api/manage/tape/delete',{library:lib,name:name});
    t.disabled=false;t.textContent='⛔ 删除';
    if(!r.ok){errEl.textContent=j.error||r.status;return;}
    if(j.warning){errEl.textContent='⚠️ '+j.warning;}
  }else{
    if(!confirm('确定初始化磁带「'+name+'」？（已写数据将丢失）'))return;
    t.disabled=true;t.textContent='初始化中…';
    const {r,j}=await jpost('/api/manage/tape/init',{library:lib,name:name});
    t.disabled=false;t.textContent='初始化';
    if(!r.ok){errEl.textContent=j.error||r.status;return;}
  }
  await loadTapeMaintainTable();
  await loadMigrateTable();
});
async function loadShelves(){
  const lib=document.getElementById('tlib').value;
  const sh=document.getElementById('tshelf');
  const mfrom=document.getElementById('mfrom');
  const mto=document.getElementById('mto');
  sh.innerHTML='';
  mfrom.innerHTML='';
  mto.innerHTML='';
  const ph=document.createElement('option');ph.value='';ph.textContent='请选择源货架';mfrom.appendChild(ph);
  const ph2=document.createElement('option');ph2.value='';ph2.textContent='请选择目标货架';mto.appendChild(ph2);
  if(!lib)return;
  const r=await fetch('/api/shelves?library='+encodeURIComponent(lib),{credentials:'include'});
  const j=await r.json();
  if(!r.ok)return;
  const shelves=j.shelves||[];
  appendShelfOptions(sh, shelves, true);
  shelves.forEach(s=>{
    const o1=document.createElement('option');o1.value=s.name;o1.textContent=s.name+(s.is_default_unused?'（未使用默认）':'');mfrom.appendChild(o1);
    const o2=document.createElement('option');o2.value=s.name;o2.textContent=s.name+(s.is_default_unused?'（未使用默认）':'');mto.appendChild(o2);
  });
}
function tapesOnShelf(list, shelfName){
  return (list||[]).filter(t=>t.shelf_name===shelfName && t.slot==null && !t.in_drive);
}
async function loadMigrateTable(){
  const me=document.getElementById('me');
  me.textContent='';
  const lib=document.getElementById('tlib').value;
  const from=document.getElementById('mfrom').value;
  const w=document.getElementById('mwrap');
  w.innerHTML='';
  if(!lib||!from){return;}
  const tr=await fetch('/api/tapes?library='+encodeURIComponent(lib),{credentials:'include'});
  const tj=await tr.json();
  if(!tr.ok){me.textContent=tj.error||tr.status;return;}
  const rows=tapesOnShelf(tj.tapes, from);
  if(!rows.length){w.innerHTML='<div class="empty">当前源货架上无可迁移磁带</div>';return;}
  let h='<table class="data-table"><thead><tr><th style="width:2.5rem"></th><th>名称</th><th>条码</th><th class="num">容量</th></tr></thead><tbody>';
  rows.forEach(t=>{h+='<tr><td><input type="checkbox" class="cm" value="'+encodeURIComponent(t.name)+'"/></td><td>'+escapeHtml(t.name)+'</td><td>'+escapeHtml(t.barcode)+'</td><td class="num">'+fmtBytes(t.capacity_bytes)+'</td></tr>';});
  h+='</tbody></table>';w.innerHTML=h;
}
document.getElementById('mfrom').addEventListener('change', loadMigrateTable);
document.getElementById('btauto').onclick=async()=>{
  document.getElementById('te').textContent='';
  const cnt=parseInt(String(document.getElementById('tcnt').value),10);
  if(!(cnt>=1&&cnt<=10000)){document.getElementById('te').textContent='数量须在 1–10000 之间';return;}
  const size=document.getElementById('tsize').value.trim();
  if(!size){document.getElementById('te').textContent='请填写容量';return;}
  const shv=document.getElementById('tshelf').value;
  const density=document.getElementById('tdensity').value;
  const szErr=checkCapacityForDensity(size,density);
  if(szErr){document.getElementById('te').textContent=szErr;return;}
  const lib=document.getElementById('tlib').value;
  const btn=document.getElementById('btauto');
  btn.disabled=true;btn.textContent='创建中（'+cnt+' 条）…';
  const {r,j}=await jpost('/api/manage/tape/create-auto-batch',{library:lib,shelf:shv?shv:null,count:cnt,size:size,density:density||null});
  btn.disabled=false;btn.textContent='创建磁带';
  if(!r.ok){document.getElementById('te').textContent=j.error||r.status;return;}
  const ns=j.names||[];
  const span=ns.length?('自 '+ns[0]+' 至 '+ns[ns.length-1]):'';
  showToast('已创建 '+ns.length+' 条 '+span);
  await loadTapeMaintainTable();
};
// --- Density capacity hints & validation ---
const DENSITY_LIMITS={
  '0x40':{label:'Default LTO',min:100*1024*1024,max:1*1024*1024*1024*1024,minH:'100 MB',maxH:'1 TB'},
  '0x4A':{label:'LTO-5',min:1*1024*1024*1024,max:3*1024*1024*1024*1024,minH:'1 GB',maxH:'3 TB'},
  '0x4C':{label:'LTO-6',min:1*1024*1024*1024,max:6*1024*1024*1024*1024,minH:'1 GB',maxH:'6 TB'},
  '0x4E':{label:'LTO-7',min:1*1024*1024*1024,max:15*1024*1024*1024*1024,minH:'1 GB',maxH:'15 TB'},
  '0x50':{label:'LTO-8',min:1*1024*1024*1024,max:30*1024*1024*1024*1024,minH:'1 GB',maxH:'30 TB'},
  '0x52':{label:'LTO-9',min:1*1024*1024*1024,max:45*1024*1024*1024*1024,minH:'1 GB',maxH:'45 TB'},
  '0x58':{label:'LTO-10',min:1*1024*1024*1024,max:90*1024*1024*1024*1024,minH:'1 GB',maxH:'90 TB'},
};
const SIZE_MULTIPLIERS={ '':1,'B':1,'K':1024,'KB':1024,'M':1048576,'MB':1048576,'G':1073741824,'GB':1073741824,'T':1099511627776,'TB':1099511627776 };
function parseSizeHuman(s){
  s=String(s).trim().toUpperCase();
  let num='',unit='';
  for(const c of s){if((c>='0'&&c<='9')||c==='.')num+=c;else unit+=c;}
  const n=parseFloat(num);
  if(isNaN(n)||n<0||!isFinite(n))return null;
  unit=unit.trim();
  const mult=SIZE_MULTIPLIERS[unit];
  if(mult===undefined)return null;
  const bytes=n*mult;
  if(!isFinite(bytes))return null;
  return Math.round(bytes);
}
function updateCapacityHint(){
  const d=document.getElementById('tdensity').value;
  const lim=DENSITY_LIMITS[d]||DENSITY_LIMITS['0x40'];
  const el=document.getElementById('tsize');
  el.placeholder=lim.minH+' - '+lim.maxH+' ('+lim.label+')';
  el.title='容量范围：'+lim.minH+' 到 '+lim.maxH;
}
function checkCapacityForDensity(sizeStr,densityCode){
  const bytes=parseSizeHuman(sizeStr);
  if(bytes===null)return '无法解析容量值：'+sizeStr;
  const lim=DENSITY_LIMITS[densityCode]||DENSITY_LIMITS['0x40'];
  if(bytes<lim.min)return '容量 '+fmtBytes(bytes)+' 小于 '+lim.label+' 格式最小 '+lim.minH;
  if(bytes>lim.max)return '容量 '+fmtBytes(bytes)+' 超过 '+lim.label+' 格式最大 '+lim.maxH;
  return null;
}
document.getElementById('tdensity').addEventListener('change',updateCapacityHint);
updateCapacityHint();
document.getElementById('mst').onclick=()=>{document.querySelectorAll('.cm').forEach(x=>{x.checked=true;});};
document.getElementById('mclr').onclick=()=>{document.querySelectorAll('.cm').forEach(x=>{x.checked=false;});};
document.getElementById('bmig').onclick=async()=>{
  const me=document.getElementById('me');
  me.textContent='';
  const lib=document.getElementById('tlib').value;
  const from=document.getElementById('mfrom').value;
  const to=document.getElementById('mto').value;
  if(!from){me.textContent='请选择源货架';return;}
  if(!to){me.textContent='请选择目标货架';return;}
  if(from===to){me.textContent='源货架与目标货架须不同';return;}
  const tapes=[...document.querySelectorAll('.cm:checked')].map(x=>decodeURIComponent(x.value));
  if(!tapes.length){me.textContent='请勾选要迁移的磁带';return;}
  const btn=document.getElementById('bmig');
  btn.disabled=true;btn.textContent='迁移中（'+tapes.length+' 条）…';
  document.getElementById('mwrap').classList.add('loading-mask');
  const {r,j}=await jpost('/api/manage/tape/migrate-shelves-batch',{library:lib,from_shelf:from,to_shelf:to,tapes:tapes});
  btn.disabled=false;btn.textContent='批量迁移';
  document.getElementById('mwrap').classList.remove('loading-mask');
  if(!r.ok){me.textContent=j.error||r.status;return;}
  showToast('已迁移 '+tapes.length+' 条');
  await loadMigrateTable();
};
loadLibs();
</script></body></html>
"#
);

pub(super) const ADMIN_LIBRARY_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — 磁带库</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
main.adm-main.adm-lib-detail{max-width:1280px;}
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_admin_side_inner.html"),
    r#"</aside>
<main class="adm-main adm-workspace adm-lib-detail">
<div class="breadcrumb">存储功能 <b>›</b> 虚拟磁带库 <b>›</b> <b>磁带库</b></div>
<div class="topbar" style="justify-content:space-between;align-items:flex-start;flex-wrap:wrap;gap:.75rem">
<div>
<h1 id="lib-title" style="margin:0;font-size:1.22rem;">磁带库</h1>
<p class="hint" style="margin:.35rem 0 0"><strong>① 磁带库</strong>：建库与几何；② 建带/入槽见工具条；③ 暴露给备份机见 <a href="/admin/transport">传输向导</a> / <a href="/admin/iscsi">iSCSI</a>。</p>
</div>
<div style="display:flex;flex-wrap:wrap;gap:.5rem;align-items:center">
<label class="ctx-label" for="libsel">当前库</label>
<select id="libsel" style="min-width:12rem"></select>
<button type="button" id="btn-create-toggle" class="btn-create" title="展开/收起新建在线库">+ 创建</button>
<span class="badge-w">已登录</span>
<button type="button" id="btn-logout">登出</button>
</div>
</div>
<div class="lib-toolbar">
<button type="button" id="tb-import" title="货架/离线区磁带装入在线库槽位">磁带入槽</button>
<button type="button" id="tb-create-tape" title="打开「磁带与货架」页的批量创建">创建磁带</button>
<button type="button" id="tb-export" title="批量离库到离线货架（与入库相反）">磁带出库</button>
<button type="button" id="tb-reconcile" title="DB 与内核机械手 inventory 对账（reconcile / auto-align）">inventory 对账</button>
<button type="button" id="tb-lun">LUN映射</button>
<button type="button" id="tb-props">属性</button>
<button type="button" id="tb-delete" class="toolbar-danger">删除</button>
</div>
<p class="err" id="le"></p>
<details class="lib-fold" id="fold-create">
<summary>新建在线库</summary>
<div class="fold-body">
<label>库名</label><input id="lname" style="max-width:24rem" pattern="[A-Za-z0-9_-]+" title="仅字母、数字、-、_"/>
<p class="hint" style="font-size:.82rem;margin:.2rem 0 0">库名即 canonical 名（与磁带目录子文件夹一致）：仅 ASCII 字母、数字、<code>-</code>、<code>_</code>；磁带名须在<strong>全部库</strong>中全局唯一，批量建带为 <code>{库名}_tape01</code>…</p>
<label>驱动数</label><input id="ldrv" type="number" value="2" min="1" max="8" style="max-width:8rem"/>
<label>槽位数</label><input id="lslot" type="number" value="32" min="1" max="256" style="max-width:8rem"/>
<p class="hint" id="lib-limits-hint" style="font-size:.82rem;color:var(--muted);margin:.35rem 0 0">产品上限：在线库最多 <strong>8</strong> 个；每库最多 <strong>8</strong> 台驱动器、<strong>256</strong> 个数据槽（与内核 vtl 一致）。</p>
<p class="hint" style="font-size:.82rem;border-left:3px solid #2980b9;padding-left:.5rem;margin:.35rem 0 0">创建/删除库后，<code>vtladm</code> 用 <code>/dev/vtl</code> ioctl 对齐 SCSI 几何（无 <code>rmmod</code>）。机械手换带由<strong>备份软件</strong>经 iSCSI 完成；Web 仅管库/磁带/货架/iSCSI。详见 <code>docs/SCSI.md</code>、<code>docs/ROBOT-SYNC.md</code>。</p>
<button type="button" id="blib">创建库</button>
<p class="hint">删除库须先选库，并在下方「删除」确认；至少保留一个在线库。</p>
</div>
</details>
<details class="lib-fold" id="fold-basic" open>
<summary>基本信息</summary>
<div class="fold-body">
<table class="sum"><tbody id="basic-rows"></tbody></table>
</div>
</details>
<details class="lib-fold" open>
<summary>驱动器</summary>
<div class="fold-body">
<table class="data-table"><thead><tr><th>驱动器</th><th>磁带</th><th>条码</th></tr></thead><tbody id="drive-rows"></tbody></table>
</div>
</details>
<details class="lib-fold" open>
<summary>磁带</summary>
<div class="fold-body">
<table class="data-table"><thead><tr><th>名称</th><th>条码</th><th>容量</th><th>已用</th><th>位置</th></tr></thead><tbody id="tape-rows"></tbody></table>
</div>
</details>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"let VTL_LIMITS={max_online_libraries:8,max_drives_per_library:8,max_data_slots_per_library:256};
function applyProductLimitsFromApi(j){
  if(!j||!j.product_limits)return;
  const L=j.product_limits;
  VTL_LIMITS={max_online_libraries:Number(L.max_online_libraries)||8,max_drives_per_library:Number(L.max_drives_per_library)||8,max_data_slots_per_library:Number(L.max_data_slots_per_library)||256};
  const d=document.getElementById('ldrv'),s=document.getElementById('lslot'),h=document.getElementById('lib-limits-hint');
  if(d){d.max=VTL_LIMITS.max_drives_per_library;d.min=1;}
  if(s){s.max=VTL_LIMITS.max_data_slots_per_library;s.min=1;}
  if(h){h.innerHTML='产品上限：在线库最多 <strong>'+VTL_LIMITS.max_online_libraries+'</strong> 个；每库最多 <strong>'+VTL_LIMITS.max_drives_per_library+'</strong> 台驱动器、<strong>'+VTL_LIMITS.max_data_slots_per_library+'</strong> 个数据槽（与内核 vtl 一致）。';}
}
function fmtBytes(n){n=Number(n)||0;if(n===0)return'0 B';const u=['B','KB','MB','GB','TB'];let i=0,x=n;while(x>=1024&&i<u.length-1){x/=1024;i++;}return (x>=100||i===0?x.toFixed(0):x.toFixed(1))+' '+u[i];}
function escapeHtml(s){return String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
function locOf(t){if(t.in_drive)return'驱动器';if(t.slot!==null&&t.slot!==undefined)return'槽位 '+t.slot;if(t.shelf_name)return escapeHtml(t.shelf_name);return'—';}
async function jpost(url,body){
  const r=await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},credentials:'include',body:JSON.stringify(body)});
  const t=await r.text();let j;try{j=JSON.parse(t);}catch{j={raw:t};}return{r,j};
}
function onlineLibs(j){return (j.libraries||[]).filter(l=>!l.is_offline_storage&&l.name);}
let onlineLibCount=0;
function setUrlLib(name){
  const u=new URL(location.href);
  u.searchParams.set('library',name);
  history.replaceState({},'',u);
}
async function loadLibSelect(){
  document.getElementById('le').textContent='';
  const r=await fetch('/api/libraries',{credentials:'include'});
  let j;
  try{j=await r.json();}catch{document.getElementById('le').textContent='库列表接口返回非 JSON';return;}
  applyProductLimitsFromApi(j);
  onlineLibCount=onlineLibs(j).length;
  const sel=document.getElementById('libsel');
  sel.innerHTML='';
  if(!r.ok){document.getElementById('le').textContent=j.error||r.status;return;}
  const params=new URLSearchParams(location.search);
  let want=params.get('library')||'';
  (j.libraries||[]).forEach(l=>{
    const o=document.createElement('option');
    o.value=l.name;
    o.textContent=l.name+(l.is_offline_storage?'（离线保留库）':'');
    sel.appendChild(o);
  });
  const names=(j.libraries||[]).map(l=>l.name);
  if(want&&names.includes(want)) sel.value=want;
  else if(sel.options.length) sel.value=sel.options[0].value;
}
async function loadDetail(){
  document.getElementById('le').textContent='';
  const lib=document.getElementById('libsel').value;
  if(!lib)return;
  setUrlLib(lib);
  const r=await fetch('/api/library/detail?library='+encodeURIComponent(lib),{credentials:'include'});
  const j=await r.json();
  if(!r.ok){document.getElementById('le').textContent=j.error||r.status;return;}
  const L=j.library||{};
  document.getElementById('lib-title').textContent='磁带库 — '+escapeHtml(L.name||'');
  const br=document.getElementById('basic-rows');
  br.innerHTML=
    '<tr><th>库 ID</th><td>'+L.id+'</td></tr>'+
    '<tr><th>名称</th><td>'+escapeHtml(L.name||'')+'</td></tr>'+
    '<tr><th>创建时间</th><td>'+escapeHtml(L.created_at||'')+'</td></tr>'+
    '<tr><th>离线保留库</th><td>'+(L.is_offline_storage?'是':'否')+'</td></tr>'+
    '<tr><th>磁带总数</th><td>'+L.tape_count+'</td></tr>'+
    '<tr><th>已装入驱动器</th><td>'+L.loaded_in_drives+' / '+L.drive_count+'</td></tr>'+
    '<tr><th>数据槽位数</th><td>'+L.data_slots+'</td></tr>'+
    '<tr><th>I/O 槽（邮筒）</th><td>'+L.mail_slots+'</td></tr>'+
    '<tr><th>配置 max_drives</th><td>'+escapeHtml(String(L.max_drives||''))+'</td></tr>'+
    '<tr><th>配置 slots</th><td>'+escapeHtml(String(L.slots||''))+'</td></tr>';
  const dr=document.getElementById('drive-rows');
  if(!j.drives||!j.drives.length){
    dr.innerHTML='<tr><td colspan="3" class="muted">无驱动器行</td></tr>';
  }else{
    dr.innerHTML=j.drives.map(d=>'<tr><td class="num">drive '+d.drive_id+'</td><td>'+(d.tape_name?escapeHtml(d.tape_name):'—')+'</td><td>'+(d.tape_barcode?escapeHtml(d.tape_barcode):'—')+'</td></tr>').join('');
  }
  const tr=document.getElementById('tape-rows');
  if(!j.tapes||!j.tapes.length){
    tr.innerHTML='<tr><td colspan="5" class="muted">无磁带</td></tr>';
  }else{
    tr.innerHTML=j.tapes.map(t=>'<tr><td>'+escapeHtml(t.name)+'</td><td>'+escapeHtml(t.barcode)+'</td><td class="num">'+fmtBytes(t.capacity_bytes)+'</td><td class="num">'+fmtBytes(t.used_bytes)+'</td><td>'+locOf(t)+'</td></tr>').join('');
  }
  const off=L.is_offline_storage;
  const canDel=!off&&L.can_delete_online;
  document.getElementById('tb-import').disabled=off;
  document.getElementById('tb-create-tape').disabled=off;
  document.getElementById('tb-export').disabled=off;
  document.getElementById('tb-reconcile').disabled=off;
  document.getElementById('tb-lun').disabled=off;
  document.getElementById('tb-delete').disabled=!canDel;
}
document.getElementById('btn-logout').onclick=async()=>{await fetch('/api/logout',{method:'POST',credentials:'include'});location.href='/login';};
document.getElementById('libsel').onchange=loadDetail;
document.getElementById('btn-create-toggle').onclick=()=>{
  const d=document.getElementById('fold-create');
  d.open=!d.open;
};
document.getElementById('tb-import').onclick=()=>{
  const lib=document.getElementById('libsel').value;
  location.href='/admin/assign-slot?library='+encodeURIComponent(lib);
};
document.getElementById('tb-create-tape').onclick=()=>{
  const lib=document.getElementById('libsel').value;
  location.href='/admin/tapes?library='+encodeURIComponent(lib)+'&tab=create';
};
document.getElementById('tb-export').onclick=()=>{
  const lib=document.getElementById('libsel').value;
  location.href='/admin/shelf-place?library='+encodeURIComponent(lib);
};
document.getElementById('tb-reconcile').onclick=()=>{
  const lib=document.getElementById('libsel').value;
  location.href='/admin/changer?library='+encodeURIComponent(lib);
};
document.getElementById('tb-lun').onclick=()=>{location.href='/admin/iscsi?library='+encodeURIComponent(document.getElementById('libsel').value);};
document.getElementById('tb-props').onclick=()=>{
  const d=document.getElementById('fold-basic');
  if(d){d.open=true;d.scrollIntoView({behavior:'smooth'});}
};
document.getElementById('tb-delete').onclick=async()=>{
  const lib=document.getElementById('libsel').value;
  const r0=await fetch('/api/library/detail?library='+encodeURIComponent(lib),{credentials:'include'});
  if(!r0.ok){document.getElementById('le').textContent='无法读取库状态';return;}
  const L=(await r0.json()).library||{};
  if(L.is_offline_storage){showToast('不可删除保留库');return;}
  if(!L.can_delete_online){showToast('须至少保留一个在线库');return;}
  const c=prompt('删除在线库「'+lib+'」。请输入库名以确认：');
  if(c!==lib)return;
  const {r,j}=await jpost('/api/manage/library/delete',{name:lib});
  if(!r.ok){document.getElementById('le').textContent=j.error||r.status;return;}
  let msg='已删除';
  if(Array.isArray(j.file_warnings)&&j.file_warnings.length) msg+='（有文件清理警告，见 JSON）';
  if(j.kernel_geom&&j.kernel_geom!=='ioctl_ok'&&j.kernel_geom!=='rescan_only'&&j.kernel_geom!=='hot_geom_disabled'&&j.kernel_geom!=='script_ok'&&j.kernel_geom!=='reload_ok'){
    msg+=' [内核:'+j.kernel_geom+(j.kernel_geom_detail?(' '+j.kernel_geom_detail):'')+']';
  } else if(j.kernel_geom==='hot_geom_disabled'){
    msg+=' [请执行 vtl-kernelctl reload]';
  }
  showToast(msg);
  await loadLibSelect();
  await loadDetail();
};
document.getElementById('blib').onclick=async()=>{
  document.getElementById('le').textContent='';
  const nm=document.getElementById('lname').value.trim();
  const drives=parseInt(document.getElementById('ldrv').value,10);
  const slots=parseInt(document.getElementById('lslot').value,10);
  if(!Number.isFinite(drives)||drives<1||drives>VTL_LIMITS.max_drives_per_library){document.getElementById('le').textContent='驱动数须在 1..'+VTL_LIMITS.max_drives_per_library;return;}
  if(!Number.isFinite(slots)||slots<1||slots>VTL_LIMITS.max_data_slots_per_library){document.getElementById('le').textContent='槽位数须在 1..'+VTL_LIMITS.max_data_slots_per_library;return;}
  if(onlineLibCount>=VTL_LIMITS.max_online_libraries){document.getElementById('le').textContent='在线库已达上限 '+VTL_LIMITS.max_online_libraries+' 个';return;}
  const {r,j}=await jpost('/api/manage/library/create',{name:nm,drives:drives,slots:slots});
  if(!r.ok){document.getElementById('le').textContent=j.error||r.status;return;}
  let msg='已创建库';
  if(j.kernel_geom&&j.kernel_geom!=='ioctl_ok'&&j.kernel_geom!=='rescan_only'&&j.kernel_geom!=='hot_geom_disabled'&&j.kernel_geom!=='script_ok'&&j.kernel_geom!=='reload_ok'){
    msg+=' [内核:'+j.kernel_geom+(j.kernel_geom_detail?(' '+j.kernel_geom_detail):'')+']';
  } else if(j.kernel_geom==='hot_geom_disabled'){
    msg+=' [请执行 vtl-kernelctl reload]';
  } else if(j.scsi_rescan==='failed'){
    msg+=' [SCSI scan 失败，请运行 vtl-scsi-scan-all-hosts.sh]';
  } else {
    msg+='（本机 lsscsi 应见 1 机械手 + '+drives+' 磁带机；iSCSI 在「传输」页另做）';
  }
  showToast(msg);
  document.getElementById('lname').value='';
  await loadLibSelect();
  if(nm&&[...document.getElementById('libsel').options].some(o=>o.value===nm)) document.getElementById('libsel').value=nm;
  await loadDetail();
};
(async()=>{
  await loadLibSelect();
  await loadDetail();
})();
</script></body></html>
"#
);

pub(super) const ADMIN_SHELF_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — 磁带架</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
main.adm-main{max-width:42rem;}
input,select{max-width:100%;}
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_admin_side_inner.html"),
    r#"</aside>
<main class="adm-main adm-workspace">
<div class="breadcrumb">存储 · 虚拟磁带库 <b>›</b> <b>磁带架</b></div>
<div class="topbar"><h1 style="margin:0;font-size:1.2rem;">磁带架（在线 / 离线）</h1><button type="button" id="btn-logout">登出</button></div>
<p class="hint" style="margin:0 0 .75rem">货架用于<strong>模拟离库后的手动保管</strong>，与在线虚拟库分离；磁带离库后进入保留库 <code>__offline__</code> 下对应货架。</p>
<section class="panel"><h2>新建离线货架</h2>
<label>货架名</label><input id="sname"/>
<button type="button" id="bshelf">创建离线货架</button><p class="err" id="se"></p>
</section>
<section class="panel"><h2>删除在线库中的货架</h2>
<p class="hint">不可删除默认「未使用」架；架上须无磁带。</p>
<label>在线库</label><select id="dblib"></select>
<label>货架</label><select id="dbsh"></select>
<button type="button" id="bdelsh">删除货架</button><p class="err" id="sde"></p>
</section>
<section class="panel"><h2>删除离线货架</h2>
<p class="hint">仅列出自建离线货架；须无磁带。</p>
<label>离线货架</label><select id="dosh"></select>
<button type="button" id="bdelosh">删除离线货架</button><p class="err" id="osde"></p>
</section>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"async function jpost(url,body){
  const r=await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},credentials:'include',body:JSON.stringify(body)});
  const t=await r.text();let j;try{j=JSON.parse(t);}catch{j={raw:t};}return{r,j};
}
function onlineLibs(j){return (j.libraries||[]).filter(l=>!l.is_offline_storage&&l.name);}
const OFFLINE_LIB='__offline__';
async function loadDelOnlineLibs(){
  const r=await fetch('/api/libraries',{credentials:'include'});
  const j=await r.json();
  const sel=document.getElementById('dblib');
  sel.innerHTML='';
  if(!r.ok)return;
  onlineLibs(j).forEach(l=>{const o=document.createElement('option');o.value=l.name;o.textContent=l.name;sel.appendChild(o);});
  sel.onchange=loadDelOnlineShelves;
  if(sel.options.length) await loadDelOnlineShelves();
}
async function loadDelOnlineShelves(){
  const lib=document.getElementById('dblib').value;
  const sh=document.getElementById('dbsh');
  sh.innerHTML='';
  if(!lib)return;
  const r=await fetch('/api/shelves?library='+encodeURIComponent(lib),{credentials:'include'});
  const j=await r.json();
  if(!r.ok)return;
  (j.shelves||[]).filter(s=>!s.is_default_unused).forEach(s=>{
    const o=document.createElement('option');o.value=s.name;o.textContent=s.name;sh.appendChild(o);
  });
}
async function loadOfflineShelvesDel(){
  const r=await fetch('/api/offline-shelves',{credentials:'include'});
  const j=await r.json();
  const sel=document.getElementById('dosh');
  sel.innerHTML='';
  if(!r.ok)return;
  (j.shelves||[]).forEach(s=>{const o=document.createElement('option');o.value=s.name;o.textContent=s.name;sel.appendChild(o);});
}
document.getElementById('btn-logout').onclick=async()=>{await fetch('/api/logout',{method:'POST',credentials:'include'});location.href='/login';};
document.getElementById('bshelf').onclick=async()=>{
  document.getElementById('se').textContent='';
  const {r,j}=await jpost('/api/manage/shelf/create-offline',{name:document.getElementById('sname').value});
  if(!r.ok){document.getElementById('se').textContent=j.error||r.status;return;}
  showToast('已创建离线货架');
  await loadOfflineShelvesDel();
};
document.getElementById('bdelsh').onclick=async()=>{
  document.getElementById('sde').textContent='';
  const lib=document.getElementById('dblib').value;
  const name=document.getElementById('dbsh').value;
  if(!name){document.getElementById('sde').textContent='没有可删除的非默认货架';return;}
  if(!confirm('确定删除货架 '+name+' ?'))return;
  const {r,j}=await jpost('/api/manage/shelf/delete',{library:lib,name:name});
  if(!r.ok){document.getElementById('sde').textContent=j.error||r.status;return;}
  showToast('已删除货架');
  await loadDelOnlineShelves();
};
document.getElementById('bdelosh').onclick=async()=>{
  document.getElementById('osde').textContent='';
  const name=document.getElementById('dosh').value;
  if(!name){document.getElementById('osde').textContent='无自建离线货架';return;}
  if(!confirm('确定删除离线货架 '+name+' ?'))return;
  const {r,j}=await jpost('/api/manage/shelf/delete',{library:OFFLINE_LIB,name:name});
  if(!r.ok){document.getElementById('osde').textContent=j.error||r.status;return;}
  showToast('已删除离线货架');
  await loadOfflineShelvesDel();
};
loadDelOnlineLibs();
loadOfflineShelvesDel();
</script></body></html>
"#
);

pub(super) const ADMIN_CHANGER_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — inventory 对账</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
.row2{display:grid;gap:.75rem;grid-template-columns:repeat(auto-fill,minmax(14rem,1fr));}
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_admin_side_inner.html"),
    r#"</aside>
<main class="adm-main adm-workspace">
<div class="breadcrumb">存储 · 虚拟磁带库 <b>›</b> <b>inventory 对账</b></div>
<div class="topbar"><h1 style="margin:0;font-size:1.2rem;">inventory 对账</h1><button type="button" id="btn-logout">登出</button></div>
<p class="hint">运行时机械手真相在 <strong>vtl.ko</strong>（与 <code>mtx</code> 一致）。本页核对 SQLite 目录与内核 inventory：<code>reconcile --pull</code> / <code>auto-align</code>；<code>sync-db</code> 仅镜像内核数据槽号到 <code>tapes.slot</code>（API <code>/api/manage/robot/sync</code>）。DB→内核全量 <code>robot sync</code> 已移除。装/卸/弹出可用 CLI 或 API（ioctl，与 assign-slot 同类）。</p>
<section class="panel"><h2>库状态</h2>
<label>在线库</label><select id="alib"></select>
<button type="button" id="reload">刷新状态</button>
<pre id="st" class="mono" style="margin-top:.5rem;white-space:pre-wrap;font-size:.85rem;"></pre>
</section>
<section class="panel"><h2>DB ↔ 内核 inventory</h2>
<button type="button" id="btnRecon">对账（仅报告）</button>
<button type="button" id="btnReconPull">写回 DB（内核→DB，pull）</button>
<button type="button" id="btnAutoAlign">自动对齐（auto-align）</button>
<p class="hint" style="font-size:.82rem;margin-top:.5rem">「写回 DB」适用于备份软件搬带后；「自动对齐」会离架撤出并在配置允许时 pull/apply。勿在 initiator 活跃时对同一库做 DB→内核 apply。</p>
<p class="err" id="robotErr"></p>
</section>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"function escapeHtml(s){return String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
async function jpost(url,body){
  const r=await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},credentials:'include',body:JSON.stringify(body)});
  const t=await r.text();let j;try{j=JSON.parse(t);}catch{j={raw:t};}return{r,j};
}
function onlineLibs(j){return (j.libraries||[]).filter(l=>!l.is_offline_storage&&l.name);}
document.getElementById('btn-logout').onclick=async()=>{await fetch('/api/logout',{method:'POST',credentials:'include'});location.href='/login';};
async function loadLibs(){
  const r=await fetch('/api/libraries',{credentials:'include'});
  const j=await r.json();
  const sel=document.getElementById('alib');
  sel.innerHTML='';
  if(!r.ok){document.getElementById('st').textContent=j.error||r.status;return;}
  onlineLibs(j).forEach(l=>{const o=document.createElement('option');o.value=l.name;o.textContent=l.name;sel.appendChild(o);});
  const qlib=new URLSearchParams(location.search).get('library');
  if(qlib&&[...sel.options].some(o=>o.value===qlib)) sel.value=qlib;
  sel.onchange=refresh;
  if(sel.options.length) await refresh();
}
async function refresh(){
  const lib=document.getElementById('alib').value;
  if(lib){
    const u=new URL(location.href);
    u.searchParams.set('library',lib);
    history.replaceState({},'',u);
  }
  if(!lib){document.getElementById('st').textContent='无在线库';return;}
  const r=await fetch('/api/library/detail?library='+encodeURIComponent(lib),{credentials:'include'});
  const j=await r.json();
  if(!r.ok){document.getElementById('st').textContent=j.error||r.status;return;}
  const L=j.library||{};
  const C=j.changer||{};
  const src=C.source||L.inventory_source||'db';
  let s='机械手状态来源: '+src+' (kernel=与 mtx/备份软件一致)\n';
  s+='驱动器 '+L.loaded_in_drives+'/'+L.drive_count+' 已装带\n';
  (C.drives||j.drives||[]).forEach(d=>{
    const lab=d.label||(d.drive_id!=null?'drive'+d.drive_id:'drive');
    const tn=d.tape_name; const bc=d.tape_barcode||d.barcode||'';
    s+='  '+lab+': '+(tn?tn+' ['+bc+']':'(空)')+'\n';
  });
  s+='\n数据槽位:\n';
  (C.data_slots||[]).forEach(r=>{
    s+='  '+r.label+': '+(r.tape_name?r.tape_name+' ['+(r.barcode||'')+']':'(空)')+'\n';
  });
  if(!(C.data_slots||[]).length){
    s+='  (无 changer 明细；见磁带页或 vtladm inventory)\n';
  }
  document.getElementById('st').textContent=s;
}
document.getElementById('reload').onclick=refresh;
function libBody(extra){return Object.assign({library:document.getElementById('alib').value},extra);}
async function doReconcile(apply,pull){
  document.getElementById('robotErr').textContent='';
  const {r,j}=await jpost('/api/manage/robot/reconcile',libBody({apply,pull}));
  if(!r.ok){document.getElementById('robotErr').textContent=j.error||r.status;return;}
  let msg='漂移 '+j.drift_count+' 项';
  if(j.inventory_truncated) msg+='（inventory 截断，结果可能不全）';
  if(apply) msg+='，已修复 '+j.fixes_applied;
  if(pull) msg+='，已写回 DB '+j.pull_updates;
  document.getElementById('robotErr').textContent=msg;
  await refresh();
}
document.getElementById('btnRecon').onclick=()=>doReconcile(false,false);
document.getElementById('btnReconPull').onclick=()=>doReconcile(false,true);
document.getElementById('btnAutoAlign').onclick=async()=>{
  document.getElementById('robotErr').textContent='';
  const {r,j}=await jpost('/api/manage/robot/auto-align',libBody({}));
  if(!r.ok){document.getElementById('robotErr').textContent=j.error||r.status;return;}
  let msg='evac='+j.evacuated+' apply='+j.fixes_applied+' pull='+j.pull_updates;
  if(j.drifts_remaining) msg+='；仍剩漂移 '+j.drifts_remaining;
  document.getElementById('robotErr').textContent=msg;
  await refresh();
};
loadLibs();
</script></body></html>
"#
);

pub(super) const ADMIN_ASSIGN_SLOT_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — 磁带入槽</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
#slotCount{font-weight:600;color:var(--accent);}
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_admin_side_inner.html"),
    r#"</aside>
<main class="adm-main adm-workspace">
<div class="breadcrumb">存储 · 虚拟磁带库 <b>›</b> <b>磁带入槽</b></div>
<div class="topbar"><h1 style="margin:0;font-size:1.2rem;">磁带入槽</h1><button type="button" id="btn-logout">登出</button></div>
<p class="hint">批量入槽会更新 SQLite 目录并通过 <code>/dev/vtl</code> ioctl 写入内核（与 mhVTL 的 <code>vtlcmd</code> 同类）；库状态/inventory 显示仍以内核为准。列出离线货架与在线库货架上、未在槽位内的磁带；提交按槽位号与磁带名升序配对。</p>
<section class="panel"><h2>批量分配到槽位</h2>
<label>在线库</label><select id="alib"></select>
<p class="hint" style="margin-top:.25rem;">当前库可用空数据槽：<span id="slotCount">—</span></p>
<div class="row-actions"><button type="button" id="sat">全选磁带</button><button type="button" id="sst">全选槽位</button><button type="button" id="clr">清除勾选</button></div>
<h3 style="font-size:.92rem;margin:.6rem 0 .35rem;">磁带</h3>
<div id="tapeWrap"></div>
<h3 style="font-size:.92rem;margin:.6rem 0 .35rem;">空槽位</h3>
<div id="slotWrap"></div>
<button type="button" id="bassign">批量入槽</button><p class="err" id="ae"></p>
</section>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"function escapeHtml(s){return String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
function slotCheckboxValue(s){
  if(typeof s==='number'&&Number.isFinite(s))return String(Math.trunc(s));
  var t=String(s).trim();
  if(/^\d+$/.test(t))return t;
  return '0';
}
async function jpost(url,body){
  const r=await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},credentials:'include',body:JSON.stringify(body)});
  const t=await r.text();let j;try{j=JSON.parse(t);}catch{j={raw:t};}return{r,j};
}
function onlineLibs(j){return (j.libraries||[]).filter(l=>!l.is_offline_storage&&l.name);}
function assignableTapes(list){return (list||[]).filter(t=>!t.in_drive && t.slot==null);}
document.getElementById('btn-logout').onclick=async()=>{await fetch('/api/logout',{method:'POST',credentials:'include'});location.href='/login';};
let lastTapes=[];let lastSlots=[];
const OFFLINE_LIB='__offline__';
async function loadLibs(){
  const r=await fetch('/api/libraries',{credentials:'include'});
  const j=await r.json();
  const sel=document.getElementById('alib');
  sel.innerHTML='';
  if(!r.ok){document.getElementById('ae').textContent=j.error||r.status;return;}
  onlineLibs(j).forEach(l=>{const o=document.createElement('option');o.value=l.name;o.textContent=l.name+' (#'+l.id+')';sel.appendChild(o);});
  const qlib=new URLSearchParams(location.search).get('library');
  if(qlib&&[...sel.options].some(o=>o.value===qlib)) sel.value=qlib;
  sel.onchange=loadDeps;
  if(sel.options.length) await loadDeps();
}
function renderTables(){
  const tw=document.getElementById('tapeWrap');
  const sw=document.getElementById('slotWrap');
  let th='<table class="data-table"><thead><tr><th style="width:2.5rem"></th><th>名称</th><th>货架 / 来源</th></tr></thead><tbody>';
  lastTapes.forEach(t=>{
    const sn=t.shelf_name?escapeHtml(t.shelf_name):'—';
    const src=t.from_offline?('离线 / '+sn):(sn+'（在线库货架）');
    const d=t.from_offline?'1':'0';
    th+='<tr><td><input type="checkbox" class="ct" data-offline="'+d+'" value="'+encodeURIComponent(t.name)+'"/></td><td>'+escapeHtml(t.name)+'</td><td>'+src+'</td></tr>';
  });
  th+='</tbody></table>';tw.innerHTML=th;
  let sh='<table class="data-table"><thead><tr><th style="width:2.5rem"></th><th>槽位号</th></tr></thead><tbody>';
  lastSlots.forEach(s=>{var disp=escapeHtml(String(s));var val=slotCheckboxValue(s);sh+='<tr><td><input type="checkbox" class="cs" value="'+val+'"/></td><td>'+disp+'</td></tr>';});
  sh+='</tbody></table>';sw.innerHTML=sh;
}
async function loadDeps(){
  document.getElementById('ae').textContent='';
  const lib=document.getElementById('alib').value;
  const [tr,er,orf]=await Promise.all([
    fetch('/api/tapes?library='+encodeURIComponent(lib),{credentials:'include'}),
    fetch('/api/empty-slots?library='+encodeURIComponent(lib),{credentials:'include'}),
    fetch('/api/tapes?library='+encodeURIComponent(OFFLINE_LIB),{credentials:'include'})
  ]);
  const tj=await tr.json();
  const ej=await er.json();
  const oj=await orf.json();
  if(!tr.ok){document.getElementById('ae').textContent=tj.error||tr.status;return;}
  if(!er.ok){document.getElementById('ae').textContent=ej.error||er.status;return;}
  if(!orf.ok){document.getElementById('ae').textContent=oj.error||orf.status;return;}
  const onlineAssignable=assignableTapes(tj.tapes);
  const offlineAssignable=assignableTapes(oj.tapes||[]);
  const offlineRows=offlineAssignable.map(t=>(Object.assign({},t,{from_offline:true})));
  const onlineRows=onlineAssignable.map(t=>(Object.assign({},t,{from_offline:false})));
  lastTapes=[...offlineRows,...onlineRows].sort((a,b)=>a.name.localeCompare(b.name,'zh'));
  lastSlots=ej.empty_slots||[];
  document.getElementById('slotCount').textContent=String(ej.empty_slot_count!=null?ej.empty_slot_count:lastSlots.length);
  renderTables();
  let warn='';
  if(!lastTapes.length) warn='没有可入槽的磁带（离线货架与当前在线库货架均无待装磁带）。';
  if(!lastSlots.length) warn=(warn?warn+' ':'')+'没有空槽位。';
  document.getElementById('ae').textContent=warn;
}
document.getElementById('sat').onclick=()=>{document.querySelectorAll('.ct').forEach(x=>x.checked=true);};
document.getElementById('sst').onclick=()=>{document.querySelectorAll('.cs').forEach(x=>x.checked=true);};
document.getElementById('clr').onclick=()=>{document.querySelectorAll('.ct,.cs').forEach(x=>x.checked=false);};
document.getElementById('bassign').onclick=async()=>{
  document.getElementById('ae').textContent='';
  const lib=document.getElementById('alib').value;
  const tapes=[...document.querySelectorAll('.ct:checked')].map(x=>({
    tape: decodeURIComponent(x.value),
    from_offline:x.getAttribute('data-offline')==='1'
  })).sort((a,b)=>a.tape.localeCompare(b.tape,'zh'));
  const slots=[...document.querySelectorAll('.cs:checked')].map(x=>parseInt(x.value,10)).sort((a,b)=>a-b);
  if(!tapes.length||!slots.length){document.getElementById('ae').textContent='请勾选磁带与空槽位';return;}
  if(tapes.length!==slots.length){document.getElementById('ae').textContent='勾选磁带数与槽位数须相同（当前 '+tapes.length+' / '+slots.length+'）';return;}
  const maxSlots=parseInt(String(document.getElementById('slotCount').textContent),10);
  if(tapes.length>maxSlots){document.getElementById('ae').textContent='超出可用空槽 '+maxSlots;return;}
  const pairs=tapes.map((t,i)=>({tape:t.tape,slot:slots[i],from_offline:t.from_offline}));
  const {r,j}=await jpost('/api/manage/tape/assign-slot-batch',{library:lib,pairs:pairs});
  if(!r.ok){document.getElementById('ae').textContent=j.error||r.status;return;}
  showToast('已批量入槽');
  await loadDeps();
};
loadLibs();
</script></body></html>
"#
);

pub(super) const ADMIN_SHELF_PLACE_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — 磁带出库</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_admin_side_inner.html"),
    r#"</aside>
<main class="adm-main adm-workspace">
<div class="breadcrumb">存储 · 虚拟磁带库 <b>›</b> <b>磁带出库</b></div>
<div class="topbar"><h1 style="margin:0;font-size:1.2rem;">磁带出库</h1><button type="button" id="btn-logout">登出</button></div>
<p class="hint">将所选磁带<strong>从在线库离库</strong>，移至下方<strong>离线货架</strong>（镜像文件会迁入离线区目录）。仅列出不在驱动中的磁带。</p>
<section class="panel"><h2>批量离库到离线货架</h2>
<label>来源在线库</label><select id="plib"></select>
<label>目标离线货架</label><select id="psh"></select>
<div class="row-actions"><button type="button" id="st">全选磁带</button><button type="button" id="clr">清除勾选</button></div>
<div id="tapeWrap"></div>
<button type="button" id="bplace">批量离库</button><p class="err" id="pe2"></p>
</section>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"function escapeHtml(s){return String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
async function jpost(url,body){
  const r=await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},credentials:'include',body:JSON.stringify(body)});
  const t=await r.text();let j;try{j=JSON.parse(t);}catch{j={raw:t};}return{r,j};
}
function onlineLibs(j){return (j.libraries||[]).filter(l=>!l.is_offline_storage&&l.name);}
let lastTapes=[];
document.getElementById('btn-logout').onclick=async()=>{await fetch('/api/logout',{method:'POST',credentials:'include'});location.href='/login';};
async function loadSh(){
  const r=await fetch('/api/offline-shelves',{credentials:'include'});
  const j=await r.json();
  const sh=document.getElementById('psh');
  sh.innerHTML='';
  if(!r.ok){document.getElementById('pe2').textContent=j.error||r.status;return;}
  if(!(j.shelves||[]).length){document.getElementById('pe2').textContent='请先在「新建货架」页创建离线货架。';return;}
  (j.shelves||[]).forEach(s=>{const o=document.createElement('option');o.value=s.name;o.textContent=s.name;sh.appendChild(o);});
}
async function loadLibs(){
  const r=await fetch('/api/libraries',{credentials:'include'});
  const j=await r.json();
  const sel=document.getElementById('plib');
  sel.innerHTML='';
  if(!r.ok){document.getElementById('pe2').textContent=j.error||r.status;return;}
  onlineLibs(j).forEach(l=>{const o=document.createElement('option');o.value=l.name;o.textContent=l.name+' (#'+l.id+')';sel.appendChild(o);});
  const qlib=new URLSearchParams(location.search).get('library');
  if(qlib&&[...sel.options].some(o=>o.value===qlib)) sel.value=qlib;
  sel.onchange=loadDeps;
  await loadSh();
  if(sel.options.length) await loadDeps();
}
function renderTapes(){
  const tw=document.getElementById('tapeWrap');
  let h='<table class="data-table"><thead><tr><th style="width:2.5rem"></th><th>磁带</th><th>位置</th></tr></thead><tbody>';
  lastTapes.forEach(t=>{
    const loc=t.slot!=null?'槽位 '+t.slot:(t.shelf_name?'货架：'+escapeHtml(t.shelf_name):'货架');
    h+='<tr><td><input type="checkbox" class="cp" value="'+encodeURIComponent(t.name)+'"/></td><td>'+escapeHtml(t.name)+'</td><td>'+loc+'</td></tr>';
  });
  h+='</tbody></table>';tw.innerHTML=h;
}
async function loadDeps(){
  document.getElementById('pe2').textContent='';
  const lib=document.getElementById('plib').value;
  const tr=await fetch('/api/tapes?library='+encodeURIComponent(lib),{credentials:'include'});
  const tj=await tr.json();
  if(!tr.ok){document.getElementById('pe2').textContent=tj.error||tr.status;return;}
  lastTapes=(tj.tapes||[]).filter(t=>!t.in_drive);
  renderTapes();
  if(!lastTapes.length) document.getElementById('pe2').textContent='当前库没有可离库的磁带（均在驱动中或库为空）。';
}
document.getElementById('st').onclick=()=>{document.querySelectorAll('.cp').forEach(x=>x.checked=true);};
document.getElementById('clr').onclick=()=>{document.querySelectorAll('.cp').forEach(x=>x.checked=false);};
document.getElementById('bplace').onclick=async()=>{
  document.getElementById('pe2').textContent='';
  const shelf=document.getElementById('psh').value;
  if(!shelf){document.getElementById('pe2').textContent='请选择离线货架';return;}
  const tapes=[...document.querySelectorAll('.cp:checked')].map(x=>decodeURIComponent(x.value));
  if(!tapes.length){document.getElementById('pe2').textContent='请勾选磁带';return;}
  const {r,j}=await jpost('/api/manage/tape/shelf-place-batch',{library:document.getElementById('plib').value,tapes:tapes,shelf:shelf});
  if(!r.ok){document.getElementById('pe2').textContent=j.error||r.status;return;}
  showToast('已离库到离线货架');
  await loadDeps();
};
loadLibs();
</script></body></html>
"#
);

pub(super) const ADMIN_ISCSI_HTML: &str = concat!(
    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>VTL — iSCSI / LUN 映射</title>
<style>
"#,
    include_str!("../web_shell.css"),
    r#"
main.adm-main{max-width:56rem;}
.data-table select{min-width:4.5rem;}
</style>
</head>
<body><div class="app">
<aside class="adm-side">"#,
    include_str!("../web_admin_side_inner.html"),
    r#"</aside>
<main class="adm-main adm-workspace">
<div class="breadcrumb">存储功能 <b>›</b> 虚拟磁带库 <b>›</b> <b>iSCSI / LUN 映射</b></div>
<div class="topbar"><div><h1 style="margin:0;font-size:1.2rem;">library-export（pscsi 多 LUN）</h1>
<p class="hint" style="margin:0.4rem 0 0;">将本机内核 <code>vtl</code> 的 <code>/dev/sg*</code> 经 LIO <strong>pscsi</strong> 导出为 iSCSI 带库（机械手 + 磁带机）。IQN 与 LIO 后端名前缀默认自动生成；下方表格可调整各 LUN 号。须先 <code>insmod vtl</code> 且 <code>lsscsi -g</code> 能看到设备。</p></div><button type="button" id="btn-logout">登出</button></div>
<section class="panel"><h2>环境与权限</h2>
<p class="hint" style="border-left:3px solid #c0392b;padding-left:.6rem;margin-bottom:.75rem"><strong>内核与主机风险：</strong>改库几何默认走 <code>/dev/vtl</code> ioctl（不卸模块）。<code>library-export</code>（LIO pscsi）与可选的 <code>kernel_vtl_reload_script</code>（<code>rmmod</code>/<code>insmod</code>，默认关闭）会触及内核；在 <code>/dev/st*</code>、<code>/dev/sg*</code> 仍被占用时 <strong>rmmod</strong> 可能导致 <strong>整机重启</strong>（麒麟 4.19 实测）。请先 <strong>dry-run</strong>、停备份；详见 <code>userspace/docs/SCSI.md</code> §1c。</p>
<p class="hint"><strong>加载配置</strong>与<strong>检测环境</strong>不修改 LIO。<code>library-export</code> / <code>library-unexport</code> 非 dry-run 须在下方<strong>开启并保存</strong>「允许执行」。成功导出会<strong>写入数据库</strong>，下次打开本页自动回填；卸除可按<strong>当前库名</strong>一键执行。门户默认取自 <code>vtl.conf</code> 的 <code>iscsi_portals</code> 首项（<strong>不支持 IPv6</strong> 字面量）。</p>
<p>
<label><input id="iallow" type="checkbox"/> 允许本页执行 <code>vtladm-iscsi</code>（非 dry-run）</label>
<button type="button" id="btn-iallow-save">保存开关</button>
<span id="iallow-msg" class="hint" style="margin-left:0.5rem;"></span>
</p>
<div id="icfg-bar" class="hint" style="white-space:pre-wrap;"></div>
<p><button type="button" id="btn-icfg">从 vtl.conf 加载推荐值</button>
<button type="button" id="btn-ichk">检测 targetcli 环境</button></p>
<label><input id="ichksudo" type="checkbox"/> 检测时使用 <code>--sudo</code></label>
<pre id="io0" style="max-height:10rem;overflow:auto;"></pre>
</section>
<section class="panel"><h2>LUN 映射与导出</h2>
<p class="hint" id="iscsi-limits-hint">产品上限：在线库最多 <strong>8</strong> 个；每库最多 <strong>8</strong> 台驱动器、<strong>256</strong> 个数据槽。导出与扫描<strong>仅使用当前库配置的驱动器台数</strong>（内核可能可见更多磁带 LUN，多余的不显示、不导出）。</p>
<p class="hint">选择<strong>在线库</strong>后点「加载默认」或「扫描 lsscsi」：均会生成 <strong>IQN</strong> 与 <strong>LIO 后端前缀</strong>，并填充 LUN 表（1 机械手 + N 台磁带机）。扫描在同一 SCSI host 上取前 N 个 <code>/dev/sg</code>（N = 库驱动器数）。</p>
<label>当前在线库</label><select id="iselib"></select>
<button type="button" id="brefreshlibs">刷新库列表</button>
<button type="button" id="bloaddef">加载默认 IQN / 门户 / LUN 表</button>
<button type="button" id="bscansg">扫描 lsscsi（VTL）</button>
<p class="err" id="iscsi-err"></p>
<p class="hint" id="iscsi-warn" style="display:none"></p>
<p class="hint" id="ilib-hint"></p>
<p class="hint" id="iexport-status" style="display:none"></p>
<label>IQN</label><input id="iiqn" style="max-width:100%"/>
<label>LIO 后端名前缀（自动生成，可改）</label><input id="iexpid" style="max-width:100%"/>
<p class="hint" style="margin-top:0">实际 pscsi 对象名为 <code id="iback-preview">…</code></p>
<label>机械手 <code>/dev/sg</code></label><input id="ichsg" placeholder="/dev/sg3"/>
<div id="idrvwrap"><p class="muted" style="margin:.35rem 0">加载默认后将出现各磁带机 <code>sg</code> 输入框。</p></div>
<table class="data-table" style="margin-top:.75rem"><thead><tr><th>名称</th><th>类型</th><th>LUN</th></tr></thead><tbody id="lunmap-body"></tbody></table>
<label>门户 IP</label><input id="iip" value="0.0.0.0"/>
<label>门户端口</label><input id="ipt" type="number" value="3260"/>
<label><input id="idry" type="checkbox" checked/> 仅 dry-run（推荐先勾选查看 targetcli 脚本）</label><br/>
<label><input id="isudo" type="checkbox"/> 使用 <code>--sudo</code></label><br/>
<button type="button" id="bdoexp">执行 library-export</button>
<pre id="io1" style="max-height:16rem;overflow:auto;"></pre>
</section>
<section class="panel"><h2>解除映射（library-unexport）</h2>
<p class="hint">使用当前 IQN、后端前缀与上表 LUN 号删除 LIO 对象（请先 dry-run 核对）。</p>
<label><input id="udry" type="checkbox" checked/> 仅 dry-run</label>
<label><input id="usudo" type="checkbox"/> 使用 <code>--sudo</code></label><br/>
<button type="button" id="bunexp">按库一键 library-unexport</button>
<button type="button" id="bunexp-adv">高级 unexport（手工 IQN/前缀）</button>
<pre id="io2" style="max-height:12rem;overflow:auto;"></pre>
</section>
<p class="hint"><a href="/admin/library">返回磁带库</a></p>
</main></div>
<script>
"#,
    include_str!("../web_boot.js"),
    r#"let VTL_LIMITS={max_online_libraries:8,max_drives_per_library:8,max_data_slots_per_library:256};
function applyProductLimitsFromApi(j){
  if(!j||!j.product_limits)return;
  const L=j.product_limits;
  VTL_LIMITS={max_online_libraries:Number(L.max_online_libraries)||8,max_drives_per_library:Number(L.max_drives_per_library)||8,max_data_slots_per_library:Number(L.max_data_slots_per_library)||256};
  const h=document.getElementById('iscsi-limits-hint');
  if(h){h.innerHTML='产品上限：在线库最多 <strong>'+VTL_LIMITS.max_online_libraries+'</strong> 个；每库最多 <strong>'+VTL_LIMITS.max_drives_per_library+'</strong> 台驱动器、<strong>'+VTL_LIMITS.max_data_slots_per_library+'</strong> 个数据槽。导出与扫描<strong>仅使用当前库配置的驱动器台数</strong>（内核可能可见更多磁带 LUN，多余的不显示、不导出）。';}
}
function bindClick(id,fn){
  const el=document.getElementById(id);
  if(el)el.onclick=fn;
  else console.error('missing #'+id);
}
function showExportOut(obj){
  const t=typeof obj==='string'?obj:JSON.stringify(obj,null,2);
  const io1=document.getElementById('io1');
  if(io1){io1.textContent=t;io1.scrollIntoView({behavior:'smooth',block:'nearest'});}
}
function parsePortalPort(){
  const raw=(document.getElementById('ipt').value||'').trim();
  const port=parseInt(raw,10);
  if(!Number.isFinite(port)||port<1||port>65535)return {ok:false,error:'门户端口须在 1–65535'};
  return {ok:true,port:port};
}
function setExportButtonEnabled(enabled,reason){
  const expBtn=document.getElementById('bdoexp');
  const warnEl=document.getElementById('iscsi-warn');
  if(!expBtn)return;
  expBtn.disabled=!enabled;
  const msg=(typeof reason==='string'&&reason)?reason:(reason!=null?String(reason):'当前库无法执行 library-export');
  if(!enabled){
    expBtn.title=msg;
    if(warnEl){warnEl.textContent=msg;warnEl.style.display='';}
  }else{
    expBtn.title='';
    if(warnEl){
      const w=warnEl.textContent||'';
      if(/library-export|驱动器数为 0|无法执行/.test(w)){
        warnEl.textContent='';
        warnEl.style.display='none';
      }
    }
  }
}
async function jpost(url,body){
  const r=await fetch(url,{method:'POST',headers:{'Content-Type':'application/json'},credentials:'include',body:JSON.stringify(body)});
  const t=await r.text();let j;try{j=JSON.parse(t);}catch{j={raw:t};}return{r,j};
}
function onlineLibs(j){
  return (j.libraries||[]).filter(l=>{
    const n=l.name||'';
    if(n==='__offline__')return false;
    if(l.is_offline_storage===true)return false;
    return !!n;
  });
}
function libFromPage(){
  const v=document.getElementById('iselib').value;
  if(v)return v;
  return new URLSearchParams(location.search).get('library')||'';
}
function ensureLibOption(name){
  const sel=document.getElementById('iselib');
  if(!name||[...sel.options].some(o=>o.value===name))return;
  const o=document.createElement('option');
  o.value=name;o.textContent=name;sel.appendChild(o);
}
function escapeHtml(s){return String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
function lunSelHtml(val){
  let h='<select class="lsel">';
  for(let i=0;i<=255;i++)h+='<option value="'+i+'"'+(i===val?' selected':'')+'>'+i+'</option>';
  h+='</select>';return h;
}
let lastDef=null;
function setBackPreview(){
  const p=document.getElementById('iexpid').value.trim();
  const dc=lastDef&&lastDef.drive_count!=null?parseInt(lastDef.drive_count,10):0;
  const drs=[];
  for(let i=0;i<dc;i++)drs.push(p+'_dr'+i);
  document.getElementById('iback-preview').textContent=p?((p+'_ch')+(drs.length?', '+drs.join(', '):'')):'（前缀）_ch、（前缀）_dr0 …';
}
function buildLunRows(lib, lunMap){
  const tb=document.getElementById('lunmap-body');
  tb.innerHTML='';
  const rows=[];
  rows.push({name:lib||'—',type:'介质变换器',lun:lunMap[0]||0});
  for(let i=1;i<lunMap.length;i++)rows.push({name:'磁带机 drive '+(i-1),type:'磁带驱动器',lun:lunMap[i]});
  rows.forEach((r,i)=>{
    const tr=document.createElement('tr');
    tr.innerHTML='<td>'+escapeHtml(r.name)+'</td><td>'+escapeHtml(r.type)+'</td><td>'+lunSelHtml(r.lun)+'</td>';
    tr.querySelector('.lsel').dataset.rowIdx=String(i);
    tb.appendChild(tr);
  });
}
function readLunMap(){
  return [...document.querySelectorAll('#lunmap-body .lsel')].map(s=>parseInt(s.value,10));
}
function buildDriveInputs(n){
  const w=document.getElementById('idrvwrap');
  w.innerHTML='';
  for(let i=0;i<n;i++){
    const lab=document.createElement('label');
    lab.textContent='磁带机 '+i+' /dev/sg';
    const inp=document.createElement('input');
    inp.id='idrv'+i;
    inp.placeholder='/dev/sg'+(4+i);
    inp.style.display='block';
    inp.style.maxWidth='100%';
    w.appendChild(lab);
    w.appendChild(inp);
  }
}
let iselibChangeBound=false;
async function loadLibs(){
  const errEl=document.getElementById('iscsi-err');
  errEl.textContent='';
  const sel=document.getElementById('iselib');
  let respOk=false,status=0,j={};
  try{
    const r=await fetch('/api/libraries',{credentials:'include'});
    status=r.status;
    respOk=r.ok;
    const t=await r.text();
    try{j=JSON.parse(t);}catch{
      errEl.textContent='库列表接口返回非 JSON'+(status===401?'（请先登录）':'');
      return false;
    }
  }catch(e){
    errEl.textContent='加载库列表失败：'+e;
    return false;
  }
  sel.innerHTML='';
  if(!respOk){
    errEl.textContent=j.error||(j.code==='setup_required'?'请先完成初始化配置（/admin/setup-init）':String(status));
    return false;
  }
  applyProductLimitsFromApi(j);
  const all=j.libraries||[];
  const libs=onlineLibs(j);
  libs.forEach(l=>{const o=document.createElement('option');o.value=l.name;o.textContent=l.name;sel.appendChild(o);});
  const q=new URLSearchParams(location.search).get('library');
  if(q){
    const names=new Set((j.libraries||[]).map(l=>l.name));
    if(names.has(q))ensureLibOption(q);
    if([...sel.options].some(o=>o.value===q))sel.value=q;
  }
  if(!sel.value&&sel.options.length)sel.value=sel.options[0].value;
  if(!libs.length){
    const db=j.db_path?(' 数据库：'+j.db_path):'';
    if(j.hint){
      errEl.innerHTML=escapeHtml(String(j.hint))+db+'. <a href="/admin/library">打开磁带库</a>';
    }else if(all.length){
      errEl.innerHTML='仅有离线保留库，不能用于 iSCSI。'+escapeHtml(db)+' <a href="/admin/library">打开磁带库</a> 创建在线库。';
    }else{
      const scsi=j.vtl_scsi_lines>0?'（lsscsi 已见 '+j.vtl_scsi_lines+' 行 VTL，但库未写入 DB）':'（未检测到 VTL SCSI）';
      errEl.innerHTML='暂无在线磁带库'+scsi+'。'+escapeHtml(db)+' 请先在 <a href="/admin/library">磁带库</a> 创建（如 marstor），再点「刷新库列表」。';
    }
    return false;
  }
  if(!iselibChangeBound){
    sel.addEventListener('change',()=>loadDefaults(false));
    iselibChangeBound=true;
  }
  return true;
}
async function loadDefaults(forceNew, noAutoRetry){
  document.getElementById('iscsi-err').textContent='';
  const warnEl=document.getElementById('iscsi-warn');
  warnEl.textContent='';
  warnEl.style.display='none';
  const lib=libFromPage();
  if(!lib){document.getElementById('iscsi-err').textContent='请选择在线库，或从磁带库页点「LUN映射」跳转';return;}
  ensureLibOption(lib);
  document.getElementById('iselib').value=lib;
  let url='/api/manage/iscsi/library-export-defaults?library='+encodeURIComponent(lib);
  if(forceNew)url+='&regenerate=1';
  const r=await fetch(url,{credentials:'include'});
  const j=await r.json();
  if(!r.ok){document.getElementById('iscsi-err').textContent=j.error||r.status;return;}
  lastDef=j;
  if(forceNew){
    warnEl.style.display='';
    warnEl.textContent='已生成新 IQN/前缀（未写入数据库）。若库内仍有导出记录，一键 unexport 仍使用旧参数；更换 IQN 前请先按库 unexport。';
  }
  document.getElementById('iiqn').value=j.iqn||'';
  document.getElementById('iexpid').value=j.export_id||'';
  document.getElementById('iip').value=j.portal_ip||'0.0.0.0';
  document.getElementById('ipt').value=j.portal_port!=null?j.portal_port:3260;
  const dc=parseInt(j.drive_count,10)||0;
  applyProductLimitsFromApi(j);
  document.getElementById('ilib-hint').textContent='库：'+escapeHtml(lib)+'，驱动器数：'+dc+'（仅显示/导出前 '+dc+' 台；上限 '+VTL_LIMITS.max_drives_per_library+'）';
  buildDriveInputs(dc);
  const lunMap=(j.default_lun_map||[]).slice(0,dc+1);
  buildLunRows(lib,lunMap.length?lunMap:(()=>{const a=[0];for(let i=1;i<=dc;i++)a.push(i);return a;})());
  if(j.changer_sg)document.getElementById('ichsg').value=j.changer_sg;
  const savedDrives=(j.drive_sg||[]).slice(0,dc);
  for(let i=0;i<Math.min(dc,savedDrives.length);i++){
    const el=document.getElementById('idrv'+i);
    if(el)el.value=savedDrives[i];
  }
  const st=document.getElementById('iexport-status');
  if(st){
    if(j.has_saved_export){
      st.style.display='';
      st.textContent='已保存导出记录'+(j.exported_at?('（'+j.exported_at+'）'):'')+(j.saved_drive_mismatch?'；驱动器台数已变，建议重新扫描 sg 后再导出':'');
    }else{
      st.style.display='none';
      st.textContent='';
    }
  }
  if(!noAutoRetry&&(!(document.getElementById('iiqn').value||'').trim()||!(document.getElementById('iexpid').value||'').trim())){
    await loadDefaults(true,true);
  }
  setBackPreview();
  if(j.can_export===false){
    setExportButtonEnabled(false,j.export_blocked_reason);
  }else{
    setExportButtonEnabled(true,null);
  }
}
async function scanSg(){
  document.getElementById('iscsi-err').textContent='';
  const lib=libFromPage();
  if(!lib){document.getElementById('iscsi-err').textContent='请先选择在线库';return;}
  ensureLibOption(lib);
  document.getElementById('iselib').value=lib;
  const r=await fetch('/api/manage/transport/scan-sg?library='+encodeURIComponent(lib)+'&transport=iscsi',{credentials:'include'});
  const j=await r.json();
  if(!r.ok){document.getElementById('iscsi-err').textContent=j.error||r.status;return;}
  applyProductLimitsFromApi(j);
  const dc=parseInt(j.drive_count,10)||0;
  if(!dc){document.getElementById('iscsi-err').textContent='当前库驱动器数为 0，请先在磁带库页建库';return;}
  await loadDefaults(false);
  if(j.changer_sg)document.getElementById('ichsg').value=j.changer_sg;
  const drives=(j.drive_sg||[]).slice(0,dc);
  buildDriveInputs(dc);
  for(let i=0;i<drives.length;i++){
    const el=document.getElementById('idrv'+i);
    if(el)el.value=drives[i];
  }
  const lunMap=[];for(let k=0;k<=dc;k++)lunMap.push(k);
  buildLunRows(lib,lunMap);
  setBackPreview();
  document.getElementById('ilib-hint').textContent='库：'+escapeHtml(lib)+'，驱动器数：'+dc+'（扫描已取前 '+drives.length+' 台 /dev/sg；已加载 IQN/LIO 前缀）';
  document.getElementById('io0').textContent=JSON.stringify(j,null,2);
}
async function loadIscsiCfg(){
  const r=await fetch('/api/manage/iscsi/config',{credentials:'include'});
  const t=await r.text();let j;try{j=JSON.parse(t);}catch{j={raw:t};}
  const bar=document.getElementById('icfg-bar');
  if(!r.ok){bar.textContent=j.error||('HTTP '+r.status);return;}
  bar.textContent='tape_dir: '+j.tape_dir+'\ntransport: '+j.transport+'\nvtladm-iscsi: '+j.vtladm_iscsi_path+'\nallow_iscsi_exec: '+(j.allow_iscsi_exec===true);
  document.getElementById('iallow').checked=(j.allow_iscsi_exec===true);
  document.getElementById('iallow-msg').textContent='';
  if(j.iscsi_portals)bar.textContent+='\niscsi_portals: '+j.iscsi_portals;
  if(j.non_unix_build)bar.textContent+='\n（当前为非 Unix 构建：请在 Linux target 上运行。）';
  const hasSaved=lastDef&&lastDef.has_saved_export===true;
  if(j.portal_ip_suggested&&!hasSaved)document.getElementById('iip').value=j.portal_ip_suggested;
  if(j.portal_port_suggested!=null&&!hasSaved)document.getElementById('ipt').value=j.portal_port_suggested;
}
async function doUnexport(byLibrary){
  document.getElementById('io2').textContent='…';
  const lib=libFromPage();
  const body={
    dry_run:document.getElementById('udry').checked,
    sudo:document.getElementById('usudo').checked
  };
  if(byLibrary){
    if(!lib){document.getElementById('io2').textContent=JSON.stringify({error:'请选择在线库'},null,2);return;}
    ensureLibOption(lib);
    document.getElementById('iselib').value=lib;
    body.library=lib;
  }else{
    body.iqn=document.getElementById('iiqn').value.trim();
    body.export_id=document.getElementById('iexpid').value.trim();
    const lunMap=readLunMap();
    if(lunMap.length)body.lun_map=lunMap;
    else if(lastDef&&lastDef.drive_count!=null)body.drives=parseInt(lastDef.drive_count,10);
  }
  const {r,j}=await jpost('/api/manage/iscsi/library-unexport',body);
  document.getElementById('io2').textContent=JSON.stringify(j,null,2);
  if(r.ok&&j.ok&&!body.dry_run)await loadDefaults(false);
}
bindClick('btn-logout',async()=>{await fetch('/api/logout',{method:'POST',credentials:'include'});location.href='/login';});
const iexpidEl=document.getElementById('iexpid');
if(iexpidEl)iexpidEl.oninput=setBackPreview;
bindClick('btn-icfg',loadIscsiCfg);
bindClick('btn-iallow-save',async()=>{
  document.getElementById('iallow-msg').textContent='…';
  const allow=document.getElementById('iallow').checked;
  const {r,j}=await jpost('/api/manage/iscsi/allow-exec',{allow:allow});
  if(!r.ok){document.getElementById('iallow-msg').textContent=j.error||('HTTP '+r.status);return;}
  document.getElementById('iallow-msg').textContent='已保存';
  await loadIscsiCfg();
});
bindClick('btn-ichk',async()=>{
  document.getElementById('io0').textContent='…';
  const {r,j}=await jpost('/api/manage/iscsi/check',{sudo:document.getElementById('ichksudo').checked});
  document.getElementById('io0').textContent=JSON.stringify(j,null,2);
});
bindClick('bloaddef',()=>loadDefaults(true));
bindClick('bscansg',scanSg);
bindClick('bunexp',()=>doUnexport(true));
bindClick('bunexp-adv',()=>doUnexport(false));
bindClick('brefreshlibs',async()=>{
  if(await loadLibs()){
    const lib=libFromPage();
    if(lib)ensureLibOption(lib);
    if(lib)await loadDefaults(false);
  }
});
bindClick('bdoexp',async()=>{
  const errEl=document.getElementById('iscsi-err');
  errEl.textContent='';
  const expBtn=document.getElementById('bdoexp');
  if(expBtn&&expBtn.disabled){
    const msg=expBtn.title||'当前库无法执行 library-export';
    showExportOut({error:msg});
    errEl.textContent=msg;
    return;
  }
  try{
    showExportOut('…');
    const lib=libFromPage();
    if(!lib){showExportOut({error:'请选择在线库'});return;}
    ensureLibOption(lib);
    document.getElementById('iselib').value=lib;
    const iqn=(document.getElementById('iiqn').value||'').trim();
    const expid=(document.getElementById('iexpid').value||'').trim();
    if(!iqn||!expid){
      showExportOut({error:'请先「加载默认」或「扫描 lsscsi」以生成 IQN 与 LIO 前缀'});
      return;
    }
    const inputs=[...document.querySelectorAll('#idrvwrap input')];
    const drives=inputs.map(i=>i.value.trim()).filter(Boolean);
    if(inputs.length&&drives.length!==inputs.length){
      showExportOut({error:'请为每个磁带机填写 /dev/sg 路径'});
      return;
    }
    const lunMap=readLunMap();
    if(lunMap.length!==1+drives.length){
      showExportOut({error:'LUN 行数须等于 1+磁带机数；请先「加载默认」'});
      return;
    }
    const pp=parsePortalPort();
    if(!pp.ok){showExportOut({error:pp.error});errEl.textContent=pp.error;return;}
    const {r,j}=await jpost('/api/manage/iscsi/library-export',{
      library:lib,
      iqn:iqn,
      export_id:expid,
      changer_sg:document.getElementById('ichsg').value.trim(),
      drive_sg:drives,
      lun_map:lunMap,
      portal_ip:document.getElementById('iip').value.trim(),
      portal_port:pp.port,
      dry_run:document.getElementById('idry').checked,
      sudo:document.getElementById('isudo').checked
    });
    showExportOut(j);
    if(!r.ok)errEl.textContent=(j&&j.error)?String(j.error):('HTTP '+r.status);
    if(r.ok&&j.ok&&!document.getElementById('idry').checked)await loadDefaults(false);
  }catch(e){
    showExportOut({error:String(e)});
    errEl.textContent=String(e);
  }
});
(async()=>{
  await loadLibs();
  await loadIscsiCfg();
  const lib=libFromPage();
  if(lib){
    ensureLibOption(lib);
    document.getElementById('iselib').value=lib;
    await loadDefaults(false);
  }
})();
</script></body></html>
"#
);
