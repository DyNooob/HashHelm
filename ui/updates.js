'use strict';
// URLs and checksums stay in the native updater; IPC only requests user actions.
window.createUpdater=function(api){
 const {$,send,page,save}=api,t=s=>window.I18n.t(s);
 let id=0,phase='idle',report=null,error='',saved='',lastAutoAttempt=0,auto=true,channel='stable',timer=null,received=0,total=null,initialized=false;
 const bytes=n=>n>=1048576?(n/1048576).toFixed(1)+' MB':n>=1024?(n/1024).toFixed(1)+' KB':n+' B';
 const errors={network:'暂时无法连接更新服务，请稍后重试。',metadata:'更新信息无效，请稍后重试。',checksum:'校验失败，下载文件已丢弃。',size:'文件大小不符，下载文件已丢弃。',io:'无法保存文件，请选择可写的文件夹。',exists:'文件已存在，请另选文件名。',withdrawn:'此更新已变更或撤回，请重新检查。',not_available:'请先检查可用的更新。',busy:'更新操作正在进行。',invalid_options:'更新设置无效。'};
 function render(){
  const busy=['checking','downloading','cancelling'].includes(phase),available=!!report?.update_available;
  $('checkUpdate').disabled=busy;$('sidebarUpdate').disabled=busy;$('updateChannel').disabled=busy;
  $('updateAvailable').classList.toggle('hidden',!available||busy||phase==='saved');
  $('updateTransfer').classList.toggle('hidden',!['downloading','cancelling'].includes(phase));
  $('updateSaved').classList.toggle('hidden',phase!=='saved');$('updateDot').classList.toggle('hidden',!available);
  $('cancelUpdate').disabled=phase==='cancelling';$('sidebarUpdateLabel').textContent=t(available?'发现新版本':phase==='checking'?'正在检查更新…':'检查更新');
  let status='尚未检查',detail='';
  if(phase==='checking')status='正在检查更新…';
  else if(phase==='downloading')status='正在下载安装包…';
  else if(phase==='cancelling')status='正在取消下载…';
  else if(phase==='saved'){status='下载完成，SHA-256 校验通过';detail='退出软件后，手动安装或解压。';}
  else if(phase==='error'){status='暂时无法完成更新';detail=errors[error]||(/http_429/.test(error)?'请求过于频繁，请稍后重试。':'更新服务暂不可用，请稍后重试。');}
  else if(phase==='cancelled')status='下载已取消';
  else if(report){
   status=({no_release:'当前已是最新版本',up_to_date:'当前已是最新版本',current_is_newer:'当前已是最新版本',new_build_available:'发现新的构建',update_available:'发现新版本'})[report.status]||'更新信息无效，请稍后重试。';
   if(available)status=t(status)+' · v'+report.latest.version+' / Build '+report.latest.build;
   if(report.update_required)detail='建议更新到此版本。';
  }
  $('updateStatus').textContent=t(status);$('updateDetail').textContent=t(detail);$('updateDetail').classList.toggle('hidden',!detail);
  if(available){const r=report.latest;$('updateNotes').textContent=r.release_notes||t('暂无更新说明');$('updateSize').textContent=(r.artifact.size_bytes?bytes(r.artifact.size_bytes)+' · ':'')+t('SHA-256 校验');}
  $('updatePath').textContent=saved;
  if(total){$('updateProgress').value=Math.min(100,received/total*100);}else $('updateProgress').removeAttribute('value');
  $('updateBytes').textContent=bytes(received)+(total?' / '+bytes(total):'');
  window.I18n.apply($('page-about'));
 }
 function check(manual=true){
  if(['checking','downloading','cancelling'].includes(phase))return;
  if(manual)page('about');
  report=null;error='';saved='';phase='checking';render();
  send('update_check',{id:++id,channel,lang:window.I18n.locale==='en'?'en':'zh'});
 }
 function schedule(startup=false){
  clearTimeout(timer);if(!auto||!initialized)return;
  const wait=startup?10000:Math.max(10000,86400000-(Date.now()-lastAutoAttempt));
  timer=setTimeout(()=>{if(!auto)return;if(['checking','downloading','cancelling'].includes(phase)){schedule();return;}lastAutoAttempt=Date.now();save();check(false);schedule();},wait);
 }
 $('checkUpdate').onclick=()=>check();$('sidebarUpdate').onclick=()=>check();
 $('downloadUpdate').onclick=()=>{if(!report?.update_available||['checking','downloading','cancelling'].includes(phase))return;phase='downloading';received=0;total=report.latest.artifact.size_bytes||null;render();send('update_download',{id:++id});};
 $('cancelUpdate').onclick=()=>{phase='cancelling';render();send('update_cancel');};
 $('openUpdateFolder').onclick=()=>send('update_open_folder');
 $('autoUpdate').onchange=()=>{auto=$('autoUpdate').checked;save();schedule(true);};
 $('updateChannel').onchange=()=>{channel=$('updateChannel').value==='beta'?'beta':'stable';report=null;phase='idle';error='';saved='';id++;render();save();};
 return {
  init(e){const p=e.config?.features?.updates;auto=p?.version===2?p.automatic!==false:true;channel=p?.channel==='beta'?'beta':'stable';lastAutoAttempt=Number.isFinite(p?.lastAutoAttempt)?Math.min(Date.now(),Math.max(0,p.lastAutoAttempt)):0;initialized=true;
   $('autoUpdate').checked=auto;$('updateChannel').value=channel;
   const version=e.version||'0.11.0';$('buildLabel').textContent='v'+version;$('aboutVersion').textContent='v'+version;
   $('aboutPlatform').textContent=({windows:'Windows',macos:'macOS',linux:'Linux'})[e.platform]||'Windows';$('aboutPlatform').textContent+=' '+(({x86_64:'x64',aarch64:'arm64'})[e.arch]||e.arch||'x64')+' · Build '+(e.build||1);
   render();schedule(true);
  },
  persist:()=>({version:2,automatic:auto,channel,lastAutoAttempt}),
  downloading:()=>['downloading','cancelling'].includes(phase),
  cancel(){if(['downloading','cancelling'].includes(phase)){phase='cancelling';render();send('update_cancel');}},
  refreshLanguage:render,
  receive(e){
   if(!['update_checked','update_error','update_progress','update_downloaded'].includes(e.type))return false;
   if(e.id!==id)return true;
   if(e.type==='update_checked'){report=e.report;phase='ready';}
   if(e.type==='update_progress'){received=e.received;total=e.total||null;}
   if(e.type==='update_downloaded'){saved=e.path;phase='saved';}
   if(e.type==='update_error'){error=e.code;phase=e.code==='cancelled'?'cancelled':'error';if(['withdrawn','metadata','not_available'].includes(error))report=null;}
   render();
   if(api.closing()&&!['downloading','cancelling'].includes(phase))send('close');
   return true;
  }
 };
};
