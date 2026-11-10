'use strict';
// Pure mask helpers: byte-oriented Hashcat masks, never a password predictor.
window.MaskTools=(()=>{
 const literal=s=>s.replaceAll('?','??');
 const classes={digit:'?d',lower:'?l',upper:'?u',symbols:'?s',any:'?a',hex:'?h',HEX:'?H',letters:'?1',alnum:'?2'};
 function infer(example,{symbols=false}={}){
  if(!example||Array.from(example).length>64||/[\x00-\x1f\x7f]/.test(example))throw Error('示例需为 1–64 个可见字符');
  return Array.from(example,c=>({char:c,kind:/[0-9]/.test(c)?'digit':/[a-z]/.test(c)?'lower':/[A-Z]/.test(c)?'upper':symbols&&/[ -~]/.test(c)?'symbols':'fixed'}));
 }
 function compose(positions){
  const mask=positions.map(p=>p.kind==='fixed'?literal(p.char):classes[p.kind]||literal(p.char)).join('');
  const customCharsets={};if(positions.some(p=>p.kind==='letters'))customCharsets.customCharset='?l?u';if(positions.some(p=>p.kind==='alnum'))customCharsets.customCharset2='?l?u?d';
  return {mask,...customCharsets};
 }
 function parse(text){
  const rows=[];
  for(const raw of text.replace(/^\uFEFF/,'').split(/\r?\n/)){
   const line=raw;if(!line.trim()||line.startsWith('#'))continue;
   const fields=[];let field='';for(let i=0;i<line.length;i++){if(line[i]==='\\'&&line[i+1]===','){field+=',';i++;}else if(line[i]===','){fields.push(field);field='';}else field+=line[i];}fields.push(field);
   if(fields.length>9)throw Error('每行最多包含八个自定义字符集和一个掩码');
   const mask=fields.pop();if(mask.length>256)throw Error('单个掩码最多 256 个字符');if(!mask)throw Error('掩码不能为空');
   const row={mask,increment:false};fields.forEach((value,i)=>row[i?'customCharset'+(i+1):'customCharset']=value);rows.push(row);
   if(rows.length>100)throw Error('一次最多使用 100 个掩码，请拆分文件');
  }
  if(!rows.length)throw Error('请至少填写一个掩码');return rows;
 }
 const library=[];let n=0;
 const add=(label,group,mask,extra={})=>library.push({id:'mask-'+(++n),label,group,mask,...extra});
 for(let len=1;len<=12;len++)add('数字 · '+len,'digits','?d'.repeat(len));
 for(let len=3;len<=12;len++)add('小写字母 · '+len,'letters','?l'.repeat(len));
 for(let len=3;len<=10;len++)add('大写字母 · '+len,'letters','?u'.repeat(len));
 for(let len=3;len<=10;len++)add('大小写字母 · '+len,'letters','?1'.repeat(len),{customCharset:'?l?u'});
 for(let len=4;len<=12;len++)add('字母与数字 · '+len,'mixed','?1'.repeat(len),{customCharset:'?l?u?d'});
 for(let len=4;len<=10;len++)add('可打印字符 · '+len,'mixed','?a'.repeat(len));
 for(let len of [4,6,8,10,12,16])add('十六进制 · '+len,'digits','?h'.repeat(len));
 add('年份 · 20xx','dates','20?d?d');add('日期形状 · YYYYMMDD','dates','?d?d?d?d?d?d?d?d');add('日期形状 · YYYY-MM-DD','dates','?d?d?d?d-?d?d-?d?d');add('日期形状 · MMDD','dates','?d?d?d?d');
 for(let len of [4,6,8]){add('小写词 + 四位数字 · '+len,'patterns','?l'.repeat(len)+'?d?d?d?d');add('首字母大写 + 四位数字 · '+len,'patterns','?u'+'?l'.repeat(len-1)+'?d?d?d?d');add('四位数字 + 小写词 · '+len,'patterns','?d?d?d?d'+'?l'.repeat(len));}
 for(let len=3;len<=8;len++){add('小写词 + 符号 · '+len,'patterns','?l'.repeat(len)+'?s');add('首字母大写 + 两位数字 · '+len,'patterns','?u'+'?l'.repeat(len-1)+'?d?d');add('小写词 + 年份 + ! · '+len,'patterns','?l'.repeat(len)+'20?d?d!');}
 add('手机号形状 · 11','patterns','1?1'+'?d'.repeat(9),{customCharset:'3456789'});
 add('CTF + 四位数字','patterns','CTF?d?d?d?d');add('首字母大写 + 数字 + 符号','patterns','?u?l?l?l?l?d?d?s');add('固定问号 + 四位数字','patterns','???d?d?d?d');
 return {infer,compose,parse,library,literal};
})();

window.createTools=api=>{
 const {$,send,toast,H}=api,t=s=>window.I18n.t(s),M=window.MaskTools;
 let target='mask',selection=null,positions=[],maskFileId=0,view='guided',dirty=false,lastFocus=null,dropTarget=null,searchIndex=0;
 const modal=$('maskDialog'),search=$('settingsDialog');
 const open=d=>{if(d.open)return;lastFocus=document.activeElement;if(d.showModal)d.showModal();else d.setAttribute('open','');};
 const close=d=>{if(d.close)d.close();else d.removeAttribute('open');lastFocus?.focus();};
 function showMasks(field='mask') {target=field;open(modal);$('maskSearch').focus();renderLibrary();}
 function preview(value){
  selection=value;const size=value?H.size(value.mask,H.charsetConfig(value)):null;
  $('maskToolPreview').textContent=value?.mask||'—';$('maskToolCount').textContent=size===null?t('请检查掩码和自定义字符集'):H.count(size)+' '+t('个候选');
  $('maskToolSets').textContent=value?Object.entries(H.charsetConfig(value)).filter(([,v])=>v).map(([n,v])=>'?'+n+' = '+v).join(' · '):'';
  $('applyMaskTool').disabled=!value||size===null;$('appendMaskTool').disabled=!value||size===null||target!=='mask';
  $('maskToolWarning').textContent=value?.group==='dates'?t('日期形状会包含无效日期；准确日期请用字典。'):size!==null&&BigInt(size)>1000000000n?t('候选范围较大，建议缩短未知部分或固定已知字符。'):'';
 }
 function renderLibrary(){
  const q=$('maskSearch').value.toLowerCase(),group=$('maskCategory').value,box=$('maskLibraryRows');box.replaceChildren();
  for(const m of M.library){const label=t(m.label);if(group&&m.group!==group||q&&!(label+' '+m.label+' '+m.mask).toLowerCase().includes(q))continue;
   const row=document.createElement('button');row.type='button';row.className='mask-library-row';row.setAttribute('role','option');row.setAttribute('aria-selected',String(selection?.id===m.id));
   for(const [tag,value]of [['span',label],['code',m.mask],['span',H.count(H.size(m.mask,H.charsetConfig(m)))]] ){const child=document.createElement(tag);child.textContent=value;row.append(child);}
   row.tabIndex=selection?.id===m.id?0:-1;
   row.onclick=()=>{preview(m);for(const child of box.children){child.setAttribute('aria-selected',String(child===row));child.tabIndex=child===row?0:-1;}row.focus({preventScroll:true});};
   row.onkeydown=event=>{const all=[...box.children],at=all.indexOf(row);let next=at;if(event.key==='ArrowDown')next=Math.min(all.length-1,at+1);else if(event.key==='ArrowUp')next=Math.max(0,at-1);else if(event.key==='Home')next=0;else if(event.key==='End')next=all.length-1;else return;event.preventDefault();all[next]?.click();all[next]?.scrollIntoView({block:'nearest'});};box.append(row);
  }
  $('maskLibraryEmpty').classList.toggle('hidden',!!box.children.length);
  if(box.children.length&&![...box.children].some(row=>row.tabIndex===0))box.firstElementChild.tabIndex=0;
 }
 function generator(){
  if(!$('maskExample').value){positions=[];$('maskPositions').replaceChildren();preview(null);$('maskToolWarning').textContent='';return;}
  try{positions=M.infer($('maskExample').value,{symbols:$('generalizeSymbols').checked});renderPositions();}catch(e){positions=[];preview(null);$('maskToolWarning').textContent=t(e.message);$('maskPositions').replaceChildren();}
 }
 function renderPositions(){
  const box=$('maskPositions');box.replaceChildren();
  const choices=[['fixed','保留原字符'],['digit','数字'],['lower','小写字母'],['upper','大写字母'],['letters','大小写字母'],['alnum','字母与数字'],['symbols','符号'],['any','可打印字符'],['hex','十六进制']];
  positions.forEach((p,i)=>{const label=document.createElement('label'),name=document.createElement('span'),select=document.createElement('select');name.textContent=(i+1)+' · '+(p.char===' '?t('空格'):p.char);label.append(name,select);select.setAttribute('aria-label',t('位置')+' '+(i+1));
   for(const [key,name]of choices){const option=document.createElement('option');option.value=key;option.textContent=t(name);select.append(option);}select.value=p.kind;
   select.onchange=()=>{p.kind=select.value;preview(M.compose(positions));};box.append(label);
  });preview(M.compose(positions));
 }
 function apply(append=false){
  if(!selection||api.guarded())return;
  if(target==='mask'){
   api.selectPattern();$('manualMask').checked=true;$('planBreadth').value='only';
   if(append){const sets=Array.from({length:8},(_,i)=>selection[i?'customCharset'+(i+1):'customCharset']||'');while(sets.length&&!sets.at(-1))sets.pop();const line=[...sets,selection.mask].map(s=>s.replaceAll(',','\\,')).join(',');$('maskList').value=[$('maskList').value,line].filter(Boolean).join('\n');}
   else{$('maskList').value='';$('mask').value=selection.mask;for(let i=0;i<8;i++)$(i?'customCharset'+(i+1):'customCharset').value=selection[i?'customCharset'+(i+1):'customCharset']||'';}
   $('mask').closest('details').open=true;api.updateScope();api.changed();
  }else{const input=$(target);input.value=selection.mask;for(let i=0;i<8;i++){const key=i?'customCharset'+(i+1):'customCharset';$(key).value=selection[key]||'';}input.dispatchEvent(new Event('input',{bubbles:true}));api.changed();}
  close(modal);$(target).focus();
 }
 function tab(name){for(const el of modal.querySelectorAll('[data-mask-tab]')){const active=el.dataset.maskTab===name;el.setAttribute('aria-selected',String(active));}for(const el of modal.querySelectorAll('[data-mask-pane]'))el.classList.toggle('hidden',el.dataset.maskPane!==name);if(name==='generate')generator();}
 $('maskSearch').oninput=renderLibrary;$('maskCategory').onchange=renderLibrary;$('maskExample').oninput=generator;$('generalizeSymbols').onchange=generator;
 $('maskSearch').onkeydown=event=>{if(event.key==='Enter'||event.key==='ArrowDown'){event.preventDefault();$('maskLibraryRows').firstElementChild?.click();}};
 $('applyMaskTool').onclick=()=>apply();$('appendMaskTool').onclick=()=>apply(true);$('closeMaskDialog').onclick=()=>close(modal);
 for(const button of document.querySelectorAll('[data-open-masks]'))button.onclick=()=>showMasks(button.dataset.openMasks||'mask');
 for(const button of modal.querySelectorAll('[data-mask-tab]'))button.onclick=()=>tab(button.dataset.maskTab);
 for(const button of document.querySelectorAll('[data-insert-mask]'))button.onclick=()=>{const field=$('mask');field.setRangeText(button.dataset.insertMask,field.selectionStart,field.selectionEnd,'end');$('manualMask').checked=true;api.updateScope();api.changed();field.focus();};
 $('maskList').oninput=()=>{if($('maskList').value.trim())$('manualMask').checked=true;api.updateScope();api.changed();};
 $('loadMaskFile').onclick=()=>send('pick',{field:'maskFile'});
 function loadMask(path){send('mask_file',{id:++maskFileId,path});}
 // Keep the same DOM fields in both layouts, so changing views cannot lose input.
 function setView(next,persist=true){view=next==='workspace'?'workspace':'guided';$('taskView').value=view;$('page-task').classList.toggle('workspace-view',view==='workspace');refresh();if(persist)api.save();}
 function refresh(){
  const state=api.read();if(!state.plan)dirty=false;
  if(view==='workspace'){
   $('step-file').classList.remove('hidden');$('step-clues').classList.remove('hidden');$('step-plan').classList.toggle('hidden',!state.plan);
   $('stepTitle').textContent=t('新建恢复任务');$('stepSubtitle').textContent=t('文件、策略和参数在同一页填写。');
   $('wizardNext').textContent=t(state.plan&&!dirty&&!state.appendMode?'开始恢复 →':'生成恢复计划');$('wizardBack').textContent=t('返回首页');
   $('enqueuePlan').disabled=state.locked||dirty;
  }
  if(view==='guided'&&state.step===2&&dirty)$('wizardNext').textContent=t('重新生成计划');
  $('planDirty').classList.toggle('hidden',!(state.plan&&dirty));
 }
 function markDirty(){if(api.read().plan)dirty=true;refresh();api.busyUI?.();}
 $('taskView').onchange=()=>{setView($('taskView').value);if(view==='guided')api.showStep(api.read().step);};
 for(const button of document.querySelectorAll('[data-jump-task]'))button.onclick=()=>$(button.dataset.jumpTask).scrollIntoView({block:'start'});
 // Native drag/drop supplies real paths; browsers cannot read File.path reliably.
 const fields=['wordlist','advancedWordlist','pcfgHints','wordlist2','composerDictionary','composerSecond','composerTarget','rules','pcfg','vcKeyfiles','sourceFile'];
 for(const id of fields){const field=$(id);if(field){const area=field.closest('.input-action')||field;area.dataset.dropField=id;}}
 $('pickTarget').dataset.dropField='target';($('homeRecover').closest('.home-start')||$('homeRecover').parentElement).dataset.dropField='target';$('maskList').dataset.dropField='maskList';
 function clearDrop(){document.querySelectorAll('.drop-hover').forEach(e=>e.classList.remove('drop-hover'));dropTarget=null;}
 function dropped(field,items){
  if(api.guarded())return;
  if(items.length>20)return toast('一次最多拖入 20 个文件');
  if(!items.length)return;
  if(field==='target'||field==='sourceFile'){
   if(items.length!==1)return toast('每次请选择一个目标文件');if(!items[0].isFile)return toast('请选择文件，不能使用文件夹');api.importDropped(items[0].path);return;
  }
  if(field==='maskList'){if(items.length!==1||!items[0].isFile)return toast('每次请选择一个掩码文本文件');loadMask(items[0].path);return;}
  if(!fields.includes(field))return;
  if(field==='pcfg'){if(items.length!==1||!items[0].isDirectory)return toast('PCFG 模型请选择文件夹');}
  else if(items.some(item=>!item.isFile))return toast('请选择文件，不能使用文件夹');
  if(!['rules','vcKeyfiles'].includes(field)&&items.length!==1)return toast('此输入框每次接收一个文件');
  $(field).value=items.map(item=>item.path).join('\n');$(field).dispatchEvent(new Event('input',{bubbles:true}));$(field).dispatchEvent(new Event('change',{bubbles:true}));
  if(field==='wordlist'){$('knownSource').value='dictionary';$('builtinDictionary').value='';}
  api.changed();toast('已填写文件路径');
 }
 function receive(e){
  if(e.type==='file_drag'){
   if(e.phase==='leave'){clearDrop();return true;}
   const area=document.elementFromPoint(e.x,e.y)?.closest('[data-drop-field]');clearDrop();if(area){dropTarget=area;area.classList.add('drop-hover');}
   if(e.phase==='drop'){const field=dropTarget?.dataset.dropField;clearDrop();if(e.count>20)toast('一次最多拖入 20 个文件');else if(field)dropped(field,e.items||[]);else toast('请拖到文件区域或路径输入框');}
   return true;
  }
  if(e.type==='mask_file'||e.type==='mask_file_error'){
   if(e.id!==maskFileId)return true;if(e.type==='mask_file_error'){toast(e.message);return true;}
   try{M.parse(e.text);api.selectPattern();$('manualMask').checked=true;$('planBreadth').value='only';$('maskList').value=e.text.replace(/^\uFEFF/,'');$('maskList').closest('details').open=true;api.updateScope();api.changed();}catch(error){toast(error.message);}return true;
  }
  if(e.type==='picked'&&e.field==='maskFile'){loadMask(e.path);return true;}
  return false;
 }
 // Search navigates to labels, never searches or displays user-entered values.
 const actions=[['新建恢复任务',()=>api.newTask(true),'Ctrl+O'],['掩码库与生成器',()=>showMasks(),'Ctrl+Shift+M'],['设备与环境',()=>api.page('devices'),''],['任务队列',()=>api.page('queue'),''],['完整命令',()=>api.page('advanced'),'']];
 function searchSettings(){
  const q=$('settingSearch').value.trim().toLowerCase(),box=$('settingResults');box.replaceChildren();let count=0;
  const result=(name,fn,section)=>{if(count++>=40)return;const b=document.createElement('button');b.className='setting-result';const label=document.createElement('span'),small=document.createElement('small');label.textContent=name;small.textContent=section;b.append(label,small);b.onclick=()=>{close(search);fn();};box.append(b);};
  for(const [name,fn,keys]of actions)if(!q||(t(name)+' '+name+' '+keys).toLowerCase().includes(q))result(t(name),fn,keys);
  if(q)for(const label of document.querySelectorAll('#page-task label[for],#page-devices label[for],#page-advanced label[for]')){
   const field=$(label.htmlFor);if(!field||field.type==='hidden'||field.closest('#vcOptions')&&$('sourceType').value!=='veracrypt')continue;const name=label.textContent.trim();if(!(name+' '+field.id).toLowerCase().includes(q))continue;
   result(name,()=>{const page=field.closest('.page');api.page(page.id.slice(5));const panel=field.closest('.step-panel');if(panel){const builder=field.closest('[data-builder]');if(builder){const attack=$('attack').value;api.selectKind(builder.dataset.builder);$('attack').value=attack;}api.showStep(['step-file','step-clues','step-plan'].indexOf(panel.id));}for(let p=field.parentElement;p;p=p.parentElement)if(p.tagName==='DETAILS')p.open=true;field.scrollIntoView({block:'center'});field.focus();},t('设置'));
  }
  $('settingEmpty').classList.toggle('hidden',!!box.children.length);
  searchIndex=0;highlightSearch();
 }
 function highlightSearch(){[...$('settingResults').children].forEach((row,i)=>row.classList.toggle('keyboard-selected',i===searchIndex));}
 function findSettings(){open(search);$('settingSearch').value='';searchSettings();$('settingSearch').focus();}
 $('findSettings').onclick=findSettings;$('closeSettingsDialog').onclick=()=>close(search);$('settingSearch').oninput=searchSettings;
 $('settingSearch').onkeydown=event=>{const rows=$('settingResults').children;if(event.key==='Enter'){event.preventDefault();rows[searchIndex]?.click();}else if(event.key==='ArrowDown'||event.key==='ArrowUp'){event.preventDefault();searchIndex=Math.max(0,Math.min(rows.length-1,searchIndex+(event.key==='ArrowDown'?1:-1)));highlightSearch();rows[searchIndex]?.scrollIntoView({block:'nearest'});}};
 document.addEventListener('keydown',event=>{
  if(!(event.ctrlKey||event.metaKey)||event.altKey)return;const key=event.key.toLowerCase();
  if(key==='k'){event.preventDefault();findSettings();}
  else if(key==='m'&&event.shiftKey){event.preventDefault();showMasks();}
  else if(key==='o'){event.preventDefault();api.newTask(true);}
  else if(event.key==='Enter'&&!modal.open&&!search.open&&$('page-task').classList.contains('active')){event.preventDefault();api.requestPlan();}
 });
 return {receive,showMasks,refresh,changed:markDirty,dirty:()=>dirty||api.read().appendMode,workspace:()=>view==='workspace',planReady:()=>{dirty=false;refresh();},
  persist:()=>({taskView:view}),init:e=>setView(e.config?.features?.interface?.taskView,false),refreshLanguage:()=>{renderLibrary();refresh();},
  dropped,
 };
};
