// Device health, preflight and queued tasks share the existing workbench views.
window.createWorkbench = api => {
 const {$,H,node,text,hide,send}=api;
 let report=null,reportFresh=false,stats={words:{},rules:{}},templates=[],benchmarks=[],measurement=null;
 let queueState={jobs:[],paused:false,maxParallel:2,activeCount:0},analysisId=0,analysis=null,probe=null,closeSent=false;
 const read=api.read,clone=x=>JSON.parse(JSON.stringify(x)),uid=prefix=>prefix+'-'+Date.now()+'-'+Math.random().toString(36).slice(2,9);
 const terminal=state=>!['queued','running'].includes(state);
 const labels={queued:'等待设备',running:'运行中',found:'已找到全部目标',exhausted:'候选耗尽',limited:'时间上限已到',stopped:'已停止',error:'运行失败',interrupted:'上次运行中断',pending:'等待尝试',skipped:'找到后已跳过'};
 const button=(label,fn,disabled=false)=>{const b=node('button',label);b.onclick=fn;b.disabled=disabled;return b;};
 const fingerprint=()=>JSON.stringify([report?.engine,(report?.raw||'').match(/hashcat \(v[^)]+\)/)?.[0],...(report?.devices||[]).map(d=>[d.id,d.name,d.backend,d.driver,d.pci])]);
 const configKey=c=>H.strategyKey(c)+'|'+fingerprint(),rateKey=c=>H.speedKey(c)+'|'+fingerprint();
 function persist(){return {templates,benchmarks:benchmarks.slice(-40)};}
 function init(e){
  templates=Array.isArray(e.config?.features?.templates)?e.config.features.templates.filter(t=>t&&typeof t.name==='string'&&Array.isArray(t.stages)).slice(0,20):[];
  benchmarks=Array.isArray(e.config?.features?.benchmarks)?e.config.features.benchmarks.filter(b=>b&&typeof b.key==='string'&&Number.isFinite(b.rate)&&b.rate>0).slice(-40):[];
  stats=e.inputStats||{words:{},rules:{}};stats.words||={};stats.rules||={};
  for(const d of read().dicts)stats.words[d.path]={count:String(d.count),basis:'bundled-catalog'};
  report=e.deviceReport||null;reportFresh=false;renderDiagnostics();renderTemplates();
  if(e.queue)receive({type:'queue_state',queue:e.queue});else send('queue_get');
 }
 function renderDiagnostics(){
  hide('diagnosticPanel',!report);if(!report)return;
  const issues=report.issues||[],errors=issues.some(i=>i.severity==='error');
  text('diagnosticStatus',errors?'检测存在错误，请先处理以下问题':issues.length?'设备可用，建议检查以下环境项':'设备检测通过');
  const panel=$('diagnosticIssues');panel.replaceChildren();
  if(!reportFresh)panel.append(node('p','上次检测结果仅供参考；重新检测后用于设备并行调度。','hint'));
  panel.append(node('small',new Date(report.checkedAt).toLocaleString(window.I18n.locale)));
  for(const issue of issues){const item=node('div',undefined,'diagnostic-finding '+issue.severity);item.append(node('strong',issue.title),node('p',issue.detail));panel.append(item);}
  const box=$('diagnosticDevices');box.replaceChildren();const selected=new Set($('devices').value.split(','));
  for(const d of report.devices||[]){const row=node('tr'),cell=node('td'),check=node('input');check.type='checkbox';check.value=String(d.id);check.checked=selected.has(String(d.id));check.setAttribute('aria-label',window.I18n.t('选择设备')+' '+d.id);cell.append(check);row.append(cell,node('td',String(d.id)));const name=node('td');name.append(node('code',d.name),node('small',d.deviceType||'GPU'));row.append(name,node('td',d.backend),node('td',d.memory||'—'));box.append(row);}
  text('deviceOutput',report.raw||'');
 }
 function diagnose(){if(api.guarded())return;api.setLock('diagnose');text('deviceOutput','正在检测设备与计算运行时…');send('diagnose',{engine:read().engine});}
 function useDevices(){
  const chosen=new Set([...$('diagnosticDevices').querySelectorAll('input:checked')].map(e=>Number(e.value))),physical=new Set(),ids=[];
  for(const d of report?.devices||[])if(chosen.has(d.id)&&!physical.has(d.physicalKey)){physical.add(d.physicalKey);ids.push(d.id);}
  $('devices').value=ids.join(',');$('deviceTypes').value='';api.toast('已采用设备选择，新建计划时生效');renderDiagnostics();
 }
 function planChanged(){
  const plan=read().plan;if(!plan)return;
  const select=$('probeStage'),selected=select.value;select.replaceChildren();plan.stages.forEach((stage,i)=>{const option=node('option',(i+1)+' · '+stage.label);option.value=String(i);select.append(option);});if(select.querySelector('option[value=\"'+selected+'\"]'))select.value=selected;text('probeOutput','');
  const box=$('workloadRows');box.replaceChildren();let known=0n,unknown=0;
  plan.stages.forEach((stage,index)=>{
   const w=H.workload(stage.config,stats,stage.count),speed=benchmarks.slice().reverse().find(b=>b.key===configKey(stage.config))||benchmarks.slice().reverse().find(b=>b.key===rateKey(stage.config));
   const seconds=w.candidates!==null&&speed?.rate>0?Number(w.candidates)/speed.rate:null;
   if(w.candidates===null)unknown++;else known+=BigInt(w.candidates);
   const row=node('tr');row.append(node('td',String(index+1)),node('td',H.count(w.base)),node('td',w.multiplier===null?'—':'× '+H.count(w.multiplier)),node('td',H.count(w.candidates)));
   const timing=node('td',seconds===null?(w.candidates===null?'范围未知':'待测速'):H.duration(seconds));
   if(speed){timing.append(node('small',speed.kind==='plan'?'按同阶段实际测速':'按算法基准，仅供参考'));timing.append(node('small',new Date(speed.at).toLocaleString(window.I18n.locale)));}
   if(w.budget)timing.append(node('small','阶段上限 '+w.budget+' 秒'));
   if(w.reasons.length){const reason=node('small',w.reasons.join('；'));reason.className='workload-reason';row.children[3].append(reason);}
   row.append(timing);box.append(row);
  });
  text('workloadSummary','已知候选 '+H.count(known.toString())+(unknown?' · '+unknown+' 个阶段范围未知':''));
  refreshBusy();
 }
 function probePlan(){
  if(api.guarded()||!read().plan||read().planStale)return;try{const stage=read().plan.stages[Number($('probeStage').value)],args=H.probeArgs(stage.config);if($('probeAction').value==='lookup'&&!$('probePassword').value)throw Error('请填写示例密码');probe={id:++analysisId,key:JSON.stringify(read().plan.stages.map(s=>H.strategyKey(s.config)))};api.setLock('probe');text('probeOutput','正在调用引擎，可随时取消…');send('plan_probe',{id:probe.id,engine:stage.config.engine,args,action:$('probeAction').value,password:$('probePassword').value});}catch(e){text('probeOutput',e.message);}
 }
 function analyze(){
  if(api.guarded()||!read().plan)return;
  analysis={id:++analysisId,key:JSON.stringify(read().plan.stages.map(s=>H.strategyKey(s.config)))};
  api.setLock('preflight');hide('preflightFeedback',false);text('preflightFeedback','正在统计输入，可随时取消…');
  send('analyze_plan',{id:analysis.id,stages:read().plan.stages.map(s=>({config:s.config}))});
 }
 function measure(kind){
  if(api.guarded())return;if(queueState.activeCount||!queueState.paused&&busy())return api.toast('队列正在使用设备，请先暂停派发并等待运行任务结束');
  let c, args;
  if(kind==='plan'){
   const stage=read().plan?.stages[0];if(!stage)return;
   c=stage.config;const measured={...c,session:uid('speed'),outfile:'',runtime:'10'};
   args=H.argv(measured,read().baseData).concat(['--speed-only','--potfile-disable','--restore-disable']);
  }else{
   c={...api.advanced(),engine:read().engine,hashMode:$('hashMode').value||'0',recipe:$('recipe').value,optimized:$('optimized').checked};
   args=[c.optimized?'-b':'-B','-m',c.hashMode];
   for(const [name,flag]of [['devices','-d'],['deviceTypes','-D'],['kernelThreads','-T'],['kernelAccel','-n'],['kernelLoops','-u'],['virtualMulti','-Y']])if(c[name])args.push(flag,c[name]);
   if(c.hashMode==='4000'&&c.recipe)args.push('--hash-recipe',c.recipe);
  }
  measurement={kind,key:kind==='plan'?configKey(c):rateKey(c),mode:c.hashMode,at:Date.now()};
  hide('benchmarkSummary',false);text('benchmarkSummary','正在测速；初始化和编译可能需要一些时间，可随时停止。');
  api.auxiliary(args,'deviceOutput',c.engine);
 }
 function finishMeasurement(e){
  if(!measurement||read().lock!=='aux')return;const sample=measurement;measurement=null;
  let frame={};for(const line of api.auxLines())frame=H.readStatus(line,frame)||frame;
  const rate=frame.speed??(frame.devices||[]).reduce((n,d)=>n+Number(d.speed||0),0);
  if(e.type==='error'||e.stopped||e.code!==0||!(rate>0))text('benchmarkSummary','测速未完成，未保存参考速度。请查看原始输出。');
  else{benchmarks.push({...sample,rate});benchmarks=benchmarks.slice(-40);text('benchmarkSummary','参考速度 '+Math.round(rate).toLocaleString()+' H/s · 算法 '+sample.mode);api.save();planChanged();}
 }
 function renderTemplates(){
  const value=$('templateSelect').value,box=$('templateSelect');box.replaceChildren();const empty=node('option','选择模板');empty.value='';box.append(empty);
  for(const t of templates){const o=node('option',t.name);o.value=t.id;box.append(o);}box.value=value;refreshBusy();
 }
 const selectedTemplate=()=>templates.find(t=>t.id===$('templateSelect').value);
 function saveTemplate(){
  const p=read().plan;if(!p||api.guarded())return;const name=$('templateName').value.trim();if(!name)return api.toast('请填写模板名称');
  if(templates.length>=20)return api.toast('最多保存 20 个模板，请先删除不再使用的模板');
  const stages=p.stages.map(s=>({label:s.label,config:Object.fromEntries(Object.entries(s.config).filter(([k])=>!['target','engine','session','outfile','count','vcPimStart','vcPimStop','vcKeyfiles','hashMode'].includes(k)))}));
  const unique=[...new Map(stages.map(s=>[H.strategyKey(s.config),s])).values()];templates.push({schema:1,id:uid('template'),name,resources:read().resourceRoot,data:read().baseData,stages:unique});api.save();renderTemplates();api.toast('策略模板已保存');
 }
 function applyTemplate(){
  if(api.guarded()||!read().plan)return;const t=selectedTemplate();if(!t)return api.toast('请先选择模板');
  try{
   const p=read().plan,moved=H.relocate(t,{oldData:t.data,newData:read().baseData,oldResources:t.resources,newResources:read().resourceRoot,windows:read().platform==='windows'});
   const id=uid('fh'),vc=p.draft?.container?.spec;let number=0;const variants=p.draft?.modes?.length?p.draft.modes:[{mode:p.mode,label:''}];const stages=moved.stages.flatMap(s=>variants.map(mode=>{const config={...s.config,engine:read().engine,target:p.target,hashMode:String(mode.mode),session:id+'-'+(++number),outfile:'',vcPimStart:vc?.pim_start||'',vcPimStop:vc?.pim_stop||'',vcKeyfiles:vc?.keyfiles?.join('\n')||''};return {label:s.label+(mode.label?' · '+mode.label:''),config,args:H.argv(config,read().baseData),state:'pending',count:null};}));if(stages.length>100)throw Error('算法组合后超过 100 个阶段，请减少掩码或缩小算法范围');
   api.replaceStages(stages);planChanged();api.toast('模板已应用，请检查计划后开始');
  }catch(e){api.toast(e.message);}
 }
 function importTemplate(t){
  if(!t||t.schema!==1||typeof t.name!=='string'||!t.name.trim()||t.name.length>80||!Array.isArray(t.stages)||!t.stages.length||t.stages.length>100)return api.toast('不是有效的 HashHelm 策略模板');
  if(templates.length>=20)return api.toast('最多保存 20 个模板，请先删除不再使用的模板');
  const allowed=new Set(['attack','hashMode','wordlist','wordlist2','mask','customCharset','customCharset2','customCharset3','customCharset4','customCharset5','customCharset6','customCharset7','customCharset8','increment','incrementMin','rules','rulesConcat','pcfg','runtime','devices','deviceTypes','kernelThreads','kernelAccel','kernelLoops','virtualMulti','statusTimer','markovDisable','recipe','optimized','slowCandidates','pcfgHints','pcfgMask','pcfgMin','pcfgMax','pcfgNoOmen']);
  const booleans=new Set(['increment','rulesConcat','markovDisable','optimized']),stages=[];for(const s of t.stages){if(typeof s.label!=='string'||!s.config||typeof s.config!=='object'||Array.isArray(s.config))return api.toast('不是有效的 HashHelm 策略模板');const pairs=[];for(const [key,value]of Object.entries(s.config)){if(!allowed.has(key))continue;if(booleans.has(key)){if(typeof value!=='boolean')return api.toast('不是有效的 HashHelm 策略模板');pairs.push([key,value]);}else if(typeof value==='string'||typeof value==='number'&&Number.isFinite(value))pairs.push([key,String(value)]);else return api.toast('不是有效的 HashHelm 策略模板');}stages.push({label:s.label.slice(0,160),config:Object.fromEntries(pairs)});}
  templates.push({schema:1,id:uid('template'),name:t.name,resources:typeof t.resources==='string'?t.resources:'',data:typeof t.data==='string'?t.data:'',stages});renderTemplates();api.save();api.toast('模板已导入，应用后请检查参数');
 }
 function enqueue(showMonitor=false,existing=null,resume=false,options={}){
  if(api.guarded())return;const p=existing||read().plan;if(!p?.stages.length)return;if(!existing&&read().planStale)return api.toast('设置已更改，请重新生成计划。');
  try{
   if(!existing&&read().groups.some(g=>g.id===p.id))return api.toast('该计划已提交，请载入计划后再次运行');
   const g=clone(p),id=existing?uid('qh'):p.id;g.id=id;g.queueId=id;g.state='queued';g.created=Date.now();g.finished=null;g.name=existing?p.name:($('reviewSession').value.trim()||p.name);g.resources=read().resourceRoot;g.data=read().baseData;if(resume)g.resumedFrom=p.id;
   g.stages=p.stages.flatMap((s,i)=>{
    const checkpoint=H.restorePath(s.args),canResume=resume&&api.hasCheckpoint(checkpoint)&&['stopped','limited','error','interrupted'].includes(s.state);
    if(resume&&(s.state!=='pending'&&!canResume||s.state==='pending'&&s.args.includes('--restore-position')&&!api.hasCheckpoint(checkpoint)))return [];
    const config=existing?{...s.config,...options,engine:read().engine,session:resume?s.config.session:id+'-'+(i+1)}:{...s.config};
    const args=resume?H.resumeArgs(s.args,options,canResume||s.args.includes('--restore-position')):existing?H.argv(config,read().baseData):s.args.slice();
    return [{label:s.label,config,args,count:s.count??null,state:'pending',ui:s.ui,strategyKind:s.strategyKind}];
   });
   if(!g.stages.length)return api.toast('没有可继续的检查点，请调整线索后重新生成计划');
   api.addGroup(g);send('queue_submit',{spec:{id,name:g.name,engine:g.stages[0].config.engine,stages:g.stages.map(s=>({label:s.label,args:s.args})),context:g}});
   if(showMonitor)api.openGroup(g);else api.page('queue');renderQueue();api.busyUI();
  }catch(e){api.toast(e.message);}
 }
 function busy(){return queueState.jobs.some(j=>!terminal(j.state))||read().groups.some(g=>g.queueId&&!terminal(g.state));}
 function renderQueue(){
  const jobs=queueState.jobs,active=jobs.filter(j=>j.state==='running').length,waiting=jobs.filter(j=>j.state==='queued').length;
  text('queueCount',active+waiting);text('queueSummary',jobs.length?active+' 个运行中 · '+waiting+' 个等待中':'队列为空');
  text('pauseQueue',queueState.paused?'继续派发':'暂停派发');$('queueParallel').value=String(queueState.maxParallel||2);$('cancelQueue').disabled=!busy();
  const box=$('queueList');box.replaceChildren();if(!jobs.length){box.append(node('div','在确认计划页加入任务，所选设备空闲时会自动开始。','empty'));return;}
  const ordered=jobs.filter(j=>!terminal(j.state)).concat(jobs.filter(j=>terminal(j.state)).slice(-20).reverse());
  for(const job of ordered){
   const g=read().groups.find(g=>g.queueId===job.spec.id),row=node('article',undefined,'queue-row'),info=node('div',undefined,'queue-info'),title=node('h2');title.textContent=job.spec.name;
   info.append(title,node('p',labels[job.state]||job.state));
   const index=job.stages.findIndex(s=>s.state==='running'),last=index>=0?index:job.stages.findLastIndex(s=>s.state!=='pending'&&s.state!=='skipped');
   if(last>=0)info.append(node('small','阶段 '+(last+1)+' / '+job.stages.length+' · '+job.spec.stages[last].label));
   info.append(node('small',job.exclusive?'独占计算设备':'已分配设备 '+(g?.stages[0]?.config.devices||'—')));
   if(job.state==='queued')info.append(node('small',queueState.paused?'已暂停派发':job.waitReason||'等待可用设备'));
   const telemetry=g?.stages[last]?.telemetry;if(telemetry)info.append(node('small',H.count(telemetry.done)+' / '+H.count(telemetry.total)+' 次候选检查'));
   const actions=node('div',undefined,'row');actions.append(button('查看任务',()=>{if(g)api.openGroup(g);},!g));
   if(job.state==='queued')for(const [label,delta,title]of [['↑',-1,'上移等待任务'],['↓',1,'下移等待任务']]){const b=button(label,()=>send('queue_control',{action:'move',id:job.spec.id,delta}));b.title=title;b.setAttribute('aria-label',window.I18n.t(title));actions.append(b);}
   if(!terminal(job.state))actions.append(button('停止此任务',()=>send('queue_control',{action:'cancel',id:job.spec.id})));
   else if(g){actions.append(button('载入计划',()=>api.reusePlan(g),read().locked));if(api.canResume(g))actions.append(button('继续任务…',()=>api.resumeGroup(g),read().locked));}
   row.append(info,actions);box.append(row);
  }
 }
 function queuedEvent(e){
  const g=read().groups.find(g=>g.queueId===e.id),s=g?.stages[e.stage];if(!s)return;const event=e.event;
  if(event.type==='stage_start'){s.telemetry=null;s.snapshots=[];s.startedAt=Date.now();s.state='running';api.logCache.set(g.id,[]);api.statusFrames.delete(s.config.session);if(read().selected?.id===g.id&&(read().inspection<0||read().inspection===e.stage)){api.resetMetrics();text('runPurpose','阶段 '+(e.stage+1)+' / '+g.stages.length+' · '+s.label);text('runStatus','正在初始化计算后端');}}
  if(event.type==='started')s.logPath=event.logPath;
  if(event.type==='lines'){
   const lines=api.logCache.get(g.id)||[];for(const line of event.lines){lines.push(line);const frame=H.readStatus(line,api.statusFrames.get(s.config.session));if(frame){api.statusFrames.set(s.config.session,frame);if(frame.done!=null)s.telemetry=frame;}}
   if(lines.length>2000)lines.splice(0,lines.length-2000);api.logCache.set(g.id,lines);s.lastOutput=Date.now();
   if(s.telemetry){const snaps=s.snapshots||=[],value=clone(s.telemetry);if(snaps.at(-1)?.at===value.at)snaps[snaps.length-1]=value;else snaps.push(value);if(snaps.length>20)snaps.shift();}
   if(read().selected?.id===g.id&&(read().inspection<0||read().inspection===e.stage)){api.renderTelemetry(s);text('runStatus',s.telemetry?'正在尝试候选密码':'正在初始化计算后端');text('runStateTag','运行中');if($('console').closest('details').open)text('console',lines.join('\n'));}
   if(Date.now()-(s.savedAt||0)>5000){s.savedAt=Date.now();api.save();}
  }
  if(['finished','error'].includes(event.type)){s.code=event.code;s.seconds=event.seconds;s.message=event.message;if(event.message){const lines=api.logCache.get(g.id)||[];lines.push(event.message);api.logCache.set(g.id,lines);}api.save();}
 }
 function receive(e){
  if(e.type==='queue_state'){
   const previous=new Map(queueState.jobs.map(j=>[j.spec.id,j.state]));queueState=e.queue;
   for(const job of queueState.jobs){
    let g=read().groups.find(g=>g.queueId===job.spec.id);
    if(!g&&job.spec.context?.stages){g=H.relocate(clone(job.spec.context),{oldData:job.spec.context.data,newData:read().baseData,oldResources:job.spec.context.resources,newResources:read().resourceRoot,windows:read().platform==='windows'});g.queueId=job.spec.id;api.addGroup(g,false);}
    if(!g)continue;g.state=job.state;g.finished=job.finishedAt;
    job.stages.forEach((state,i)=>{if(g.stages[i])Object.assign(g.stages[i],Object.fromEntries(Object.entries(state).filter(([,v])=>v!==null)));});
    if(read().selected?.id===g.id&&terminal(g.state)&&read().inspection<0){text('heartbeat','该任务已经停止计算。');$('speed').previousElementSibling.textContent='最后报告的速度';const last=g.stages.filter(s=>s.seconds!=null).at(-1);if(last)text('elapsed',Math.ceil(last.seconds)+' 秒');}
    if(read().selected?.id===g.id){api.renderMonitor();if(read().inspection<0){text('runStateTag',labels[g.state]||g.state);text('runStatus',g.state==='found'?(g.verification?'原文件验证通过':'引擎已找到候选密码'):g.state==='queued'?'等待设备':g.state==='running'?(g.stages.find(s=>s.state==='running')?.telemetry?'正在尝试候选密码':'正在初始化计算后端'):labels[g.state]||g.state);$('runStateTag').className='tag '+(g.state==='found'?'success':g.state==='error'?'error':'');}hide('outcomeCard',!terminal(g.state)||g.state==='found');text('outcomeTitle',labels[g.state]||g.state);text('outcomeNote',g.state==='limited'?'时间到不代表该阶段的候选已全部尝试。可以扩大时间上限，或在任务记录中查看检查点。':g.state==='exhausted'?'没有在本次候选范围内找到全部密码；这不代表文件没有密码。':g.state==='error'?'展开原始日志查看具体错误；更改后可以重新生成计划。':'最近检查点之后的进度可能丢失，继续时沿用原策略。');}
    if(previous.has(job.spec.id)&&!terminal(previous.get(job.spec.id))&&terminal(job.state)&&read().selected?.id===g.id&&$('page-monitor').classList.contains('active')&&!read().locked&&!read().closing)api.queryResults(g);
   }
   renderQueue();api.busyUI();api.save();api.refreshCheckpoints();
   if(read().closing&&!busy()&&!read().locked&&!closeSent){closeSent=true;send('close');}return true;
  }
  if(e.type==='queue_event'){queuedEvent(e);return true;}
  if(e.type==='queue_error'){const g=read().groups.find(g=>g.queueId===e.id);if(g){g.state='error';g.stages[0].state='error';g.stages[0].message=e.message;api.logCache.set(g.id,[e.message]);if(read().selected?.id===g.id){api.renderMonitor();text('runStatus','当前尝试未能完成');text('runStateTag','运行失败');text('heartbeat','该任务已经停止计算。');hide('outcomeCard',false);text('outcomeTitle','请检查文件、参数或设备环境');text('outcomeNote',e.message);}api.save();}api.toast(e.message);api.busyUI();return true;}
  if(e.type==='diagnosed'||e.type==='diagnose_error'){
   api.setLock('');if(e.type==='diagnosed'){report=e.report;reportFresh=true;api.setAvailable(true);renderDiagnostics();api.save();planChanged();}else {text('deviceOutput',e.message);api.toast(e.message);}if(read().closing)send('close');return true;
  }
  if(e.type==='plan_probe_done'||e.type==='plan_probe_error'){if(e.id!==probe?.id)return true;const current=probe.key===JSON.stringify(read().plan?.stages.map(s=>H.strategyKey(s.config)));probe=null;api.setLock('');text('probeOutput',current?(e.output||e.message):'计划已更改，请重新检查');if(read().closing)send('close');return true;}
  if(e.type==='preflight_done'||e.type==='preflight_error'){
   if(e.id!==analysis?.id)return true;api.setLock('');
   if(e.type==='preflight_done'&&analysis.key===JSON.stringify(read().plan?.stages.map(s=>H.strategyKey(s.config)))){Object.assign(stats.words,e.stats.words);Object.assign(stats.rules,e.stats.rules);text('preflightFeedback','输入统计已更新');planChanged();}else text('preflightFeedback',e.message||'计划已更改，请重新统计');analysis=null;if(read().closing)send('close');return true;
  }
  if(e.type==='template_file'){importTemplate(e.template);return true;}
  if(e.type==='finished'||e.type==='error')finishMeasurement(e);
  if(e.type==='rejected'){measurement=null;analysis=null;}
  return false;
 }
 function refreshBusy(){
  const state=read(),locked=state.locked||state.planStale;hide('enqueuePlan',state.step!==2||!state.plan);$('enqueuePlan').disabled=locked;
  for(const id of ['probePlan','analyzePlan','measurePlan','saveTemplate','applyTemplate','deleteTemplate','exportTemplate','importTemplate'])$(id).disabled=locked||(!state.plan&&id!=='importTemplate');
 }
 $('probePlan').onclick=probePlan;$('probeAction').onchange=()=>hide('probePasswordField',$('probeAction').value!=='lookup');
 $('enqueuePlan').onclick=()=>enqueue(false);$('queueNewTask').onclick=()=>api.newTask(true);
 $('pauseQueue').onclick=()=>send('queue_control',{action:queueState.paused?'resume':'pause'});$('queueParallel').onchange=()=>send('queue_control',{action:'limit',limit:Number($('queueParallel').value)});
 $('cancelQueue').onclick=()=>{if(api.ask('停止队列中的全部任务？最近检查点之后的进度可能丢失。'))send('queue_control',{action:'cancel_all'});};
 $('analyzePlan').onclick=analyze;$('measurePlan').onclick=()=>measure('plan');$('saveTemplate').onclick=saveTemplate;$('applyTemplate').onclick=applyTemplate;
 $('deleteTemplate').onclick=()=>{const t=selectedTemplate();if(t&&api.ask('删除所选策略模板？')){templates=templates.filter(x=>x.id!==t.id);renderTemplates();api.save();}};
 $('exportTemplate').onclick=()=>{const t=selectedTemplate();if(t)send('export',{kind:'template',text:JSON.stringify(t,null,2)});else api.toast('请先选择模板');};$('importTemplate').onclick=()=>{if(!api.guarded())send('import_template');};
 $('exportDiagnostics').onclick=()=>{if(report)send('export',{kind:'diagnostic',text:JSON.stringify({...report,platform:read().platform,exportedAt:Date.now()},null,2)});};$('useDevices').onclick=useDevices;
 setInterval(()=>{const s=read(),g=s.selected;if(!g?.queueId||g.state!=='running')return;const stage=s.inspection>=0?g.stages[s.inspection]:g.stages.find(x=>x.state==='running');if(!stage||stage.state!=='running')return;const start=stage.startedAt||Date.now(),age=Math.floor((Date.now()-(stage.telemetry?.at||stage.lastOutput||start))/1000);text('elapsed',Math.max(0,Math.floor((Date.now()-start)/1000))+' 秒');text('heartbeat',stage.telemetry?'最近状态更新于 '+age+' 秒前'+(age>10?' · 等待引擎新状态，耗时不等于卡死':' · 正在本机计算'):'正在初始化后端 / 编译内核 · 最近输出 '+age+' 秒前。可展开原始日志查看进度。');if($('page-queue').classList.contains('active'))renderQueue();},1000);
 return {init,receive,persist,planChanged,refreshBusy,renderQueue,diagnose,measure,enqueue,busy,
  cancel:id=>send('queue_control',{action:'cancel',id}),stopAll:()=>send('queue_control',{action:'cancel_all'}),
  resume:(g,options)=>enqueue(true,g,true,options),
 };
};
