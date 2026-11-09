// Pure helpers shared by command preview, execution and tests.
window.HC = (() => {
 const names={'0':'字典尝试','1':'双字典组合','3':'规律枚举','4':'概率模型','6':'字典 + 规律','7':'规律 + 字典','9':'关联尝试','12':'规律嵌入字典'};
 const fileName=p=>String(p||'').split(/[\\/]/).pop()||'未选择文件';
 function validate(c){
  if(!c.engine||!c.target)throw Error('请先选择文件并确认引擎位置');
  if(!/^\d+$/.test(c.hashMode))throw Error('请确认哈希算法编号');
  if(!names[c.attack])throw Error('此方式请在完整命令中配置');
  if(!/^[\w-]{1,90}$/.test(c.session))throw Error('内部任务编号无效，请重新生成计划');
  if(c.devices&&!/^[1-9]\d*(,[1-9]\d*)*$/.test(c.devices))throw Error('设备编号请填写数字，以英文逗号分隔');
  if(c.deviceTypes&&!/^[123](,[123])*$/.test(c.deviceTypes))throw Error('设备类型只能为 1、2、3，以英文逗号分隔');
  for(const key of ['kernelThreads','kernelAccel','kernelLoops','virtualMulti','statusTimer'])if(c[key]&&!/^[1-9]\d{0,5}$/.test(String(c[key])))throw Error('内核、并行与刷新参数需为正整数，最多六位');
  if(c.runtime&&(!/^\d+$/.test(String(c.runtime))||Number(c.runtime)<1||Number(c.runtime)>86400))throw Error('每阶段时间需为 1–86400 秒');
  if(c.attack==='4'&&!c.runtime)throw Error('概率模型需要阶段时间上限');
  if(['0','1','6','7','9','12'].includes(c.attack)&&!c.wordlist)throw Error('请选择字典或填写候选密码');
  if(c.attack==='1'&&!c.wordlist2)throw Error('双字典组合需要两份字典');
  if(['3','6','7','12'].includes(c.attack)&&!c.mask)throw Error('请描述密码规律');
  if(['3','6','7','12'].includes(c.attack)){
   const sets=charsetConfig(c),visit=(s,seen=[])=>{for(let i=0;i<s.length;i++)if(s[i]==='?'){const t=s[++i];if(/[1-8]/.test(t||'')&&t){if(!sets[t])throw Error('请填写掩码引用的自定义字符集');if(seen.includes(t))throw Error('自定义字符集不能循环引用');visit(sets[t],[...seen,t]);}else if(!'dlusahHb?'.includes(t||'')||!t){if(c.attack!=='12'||!['w','q'].includes(t)||t==='q'&&!c.wordlist2)throw Error('掩码包含不支持的占位符');}}};visit(c.mask);
  }
  if(c.hashMode==='4000'&&!c.recipe)throw Error('Hash Recipe 模式需要填写表达式');
  if(c.rules&&['1','4'].includes(c.attack))throw Error('该方式的规则参数请在完整命令中配置');
  if(c.increment&&['3','6','7'].includes(c.attack)&&(!Number.isInteger(Number(c.incrementMin))||Number(c.incrementMin)<1))throw Error('递增起始长度无效');
  for(const v of Object.values(c))if(typeof v==='string'&&v.includes('\0'))throw Error('参数中不能包含空字符');
 }
 function argv(c,data){validate(c);const m=c.attack,r=data.replaceAll('\\','/');
  const a=['-m',c.hashMode,'-a',m,'--session',c.session,'--restore-file-path',r+'/'+c.session+'.restore','--cache-path',r+'/cache','--potfile-path',r+'/recovered.potfile','--logfile-disable','--status','--status-timer',String(c.statusTimer||'1')];
  if(c.statusJson)a.push('--status-json');
  for(const [key,flag]of [['deviceTypes','-D'],['kernelThreads','-T'],['kernelAccel','-n'],['kernelLoops','-u'],['virtualMulti','-Y']])if(c[key])a.push(flag,String(c[key]));
  if(c.markovDisable)a.push('--markov-disable');
  if(c.devices)a.push('-d',c.devices);if(c.runtime)a.push('--runtime',String(c.runtime));if(c.outfile)a.push('-o',c.outfile);if(c.hashMode==='4000'&&c.recipe)a.push('--hash-recipe',c.recipe);if(c.optimized)a.push('-O');for(const rule of (c.rules||'').split(/\r?\n/).map(s=>s.trim()).filter(Boolean))a.push('-r',rule);if(c.rulesConcat&&c.rules)a.push('--rules-concat');if(m==='4')a.push('-S');
  if(['3','6','7','12'].includes(m))for(const [key,value]of Object.entries(charsetConfig(c)))if(value)a.push('-'+key,value);if(['3','6','7'].includes(m)&&c.increment)a.push('--increment','--increment-min',String(c.incrementMin));
  a.push(c.target);if(['0','1','6','9'].includes(m))a.push(c.wordlist);if(m==='1')a.push(c.wordlist2);if(['3','6'].includes(m))a.push(c.mask);if(m==='7')a.push(c.mask,c.wordlist);if(m==='12'){a.push(c.mask,c.wordlist);if(c.wordlist2)a.push(c.wordlist2);}if(m==='4'&&c.pcfg)a.push(c.pcfg);return a;
 }
 const powershell=(exe,args)=>'& '+[exe,...args].map(v=>"'"+String(v).replaceAll("'","''")+"'").join(' ');
 const posix=(exe,args)=>[exe,...args].map(v=>"'"+String(v).replaceAll("'", "'\"'\"'")+"'").join(' ');
 const command=(exe,args,platform='windows')=>platform==='windows'?powershell(exe,args):posix(exe,args);
 function decodeHex(hex){if(typeof hex!=='string'||hex.length%2||!/^[a-f0-9]*$/i.test(hex))return null;try{return new TextDecoder('utf-8',{fatal:true}).decode(Uint8Array.from(hex.match(/../g)||[],v=>parseInt(v,16)));}catch{return null;}}
 const charsetConfig=c=>Object.fromEntries(Array.from({length:8},(_,i)=>[String(i+1),i?c['customCharset'+(i+1)]||'':c.customCharset||'']));
 function size(mask,custom='',min=null){const sets={d:'0123456789',l:'abcdefghijklmnopqrstuvwxyz',u:'ABCDEFGHIJKLMNOPQRSTUVWXYZ',s:' !"#$%&\'()*+,-./:;<=>?@[\\]^_`{|}~',h:'0123456789abcdef',H:'0123456789ABCDEF',b:Array.from({length:256},(_,i)=>String.fromCharCode(i)).join('')};sets.a=sets.d+sets.l+sets.u+sets.s;Object.assign(sets,typeof custom==='object'?custom:{'1':custom});
  const expand=(s,seen=[])=>{if(!s||/[^\x20-\x7e]|\.hcchr$/i.test(s))return null;let out='';for(let i=0;i<s.length;i++){if(s[i]==='?'){const t=s[++i];if(t==='?'){out+='?';continue;}if(!sets[t]||seen.includes(t))return null;const value=/^[1-8]$/.test(t)?expand(sets[t],[...seen,t]):sets[t];if(value===null)return null;out+=value;}else out+=s[i];}return out;};let product=1n,total=0n,n=0;const widths=[];
  for(let i=0;i<mask.length;i++){let w=1;if(mask[i]==='?'){const t=mask[++i];if(/^[1-8]$/.test(t||'')){const value=expand(sets[t],[t]);if(!value)return null;w=new Set(value).size;}else if(t==='?')w=1;else if(sets[t])w=sets[t].length;else return null;}widths.push(BigInt(w));}
  for(const w of widths){product*=w;if(++n>=Number(min??widths.length))total+=product;}return total.toString();
 }
 const restorePath=args=>{const i=args?.indexOf('--restore-file-path');return i>=0?args[i+1]:'';};
 function rebase(value,oldData,newData){
  if(!oldData||!newData)return value;const oldRoot=oldData.replaceAll('\\','/').replace(/\/workspace\/?$/i,'')+'/',newRoot=newData.replaceAll('\\','/').replace(/\/workspace\/?$/i,'')+'/';
  const visit=v=>{if(typeof v==='string'){const p=v.replaceAll('\\','/');return p.slice(0,oldRoot.length).toLowerCase()===oldRoot.toLowerCase()?newRoot+p.slice(oldRoot.length):v;}if(Array.isArray(v))return v.map(visit);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,visit(x)]));return v;};return visit(value);
 }
 function relocate(value,{oldData,newData,oldResources,newResources,windows=false}){
  const norm=p=>String(p||'').replaceAll('\\','/').replace(/\/+$/,'');
  const pairs=[[oldData,newData],[oldResources,newResources]].filter(([a,b])=>a&&b).map(([a,b])=>[norm(a),norm(b)]).sort((a,b)=>b[0].length-a[0].length);
  const visit=v=>{if(typeof v==='string'){const path=norm(v);for(const [old,next]of pairs){const p=windows?path.toLowerCase():path,o=windows?old.toLowerCase():old;if(p===o||p.startsWith(o+'/'))return next+path.slice(old.length);}return v;}if(Array.isArray(v))return v.map(visit);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,visit(x)]));return v;};return visit(value);
 }
 // Each length is a separate fixed mask. --increment would truncate a known suffix.
 function patterns({min,max,charset,prefix='',suffix='',manual,mask,customCharsets={}}){
  if(manual){if(!mask)throw Error('请输入自定义掩码');return [{mask,...customCharsets,increment:false}];}
  if(!charset)throw Error('请至少选择一种字符');
  if(!Number.isInteger(min)||!Number.isInteger(max)||min<1||max<min||max>32)throw Error('长度范围需在 1–32 之间，最长不能小于最短');
  const literal=s=>s.replaceAll('?','??');return Array.from({length:max-min+1},(_,i)=>({mask:literal(prefix)+'?1'.repeat(min+i)+literal(suffix),customCharset:charset,increment:false,length:min+i}));
 }
 const integer=v=>typeof v==='string'&&/^\d+$/.test(v)?v:typeof v==='number'&&Number.isSafeInteger(v)&&v>=0?String(v):null;
 function readStatus(line,previous={}){
  // Quote integer tokens before JSON.parse: Hashcat's uint64 counters exceed JS's safe range.
  if(line.trim().startsWith('{')){try{const x=JSON.parse(line.replace(/"(?:\\.|[^"\\])*"|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/g,t=>/^\d+$/.test(t)?'"'+t+'"':t));if(!Array.isArray(x.progress))return null;
   return {done:integer(x.progress[0]),total:integer(x.progress[1]),rejected:integer(x.rejected),restore:integer(x.restore_point),recovered:x.recovered_hashes?.map(String).join(' / '),base:x.guess?.guess_base||'',mod:x.guess?.guess_mod||'',queue:x.guess?x.guess.guess_base_offset+' / '+x.guess.guess_base_count:'',devices:(x.devices||[]).map(d=>({...d,speed:Number(d.speed),temp:Number(d.temp??-1),util:Number(d.util??-1)})),windows:[],eta:Number(x.estimated_stop)>1?Number(x.estimated_stop)*1000:null,at:Date.now(),json:true};
  }catch{return null;}}
  const m=line.match(/^([A-Za-z][\w.#*]*?)\.+:\s*(.*)$/);if(!m)return null;const key=m[1],value=m[2];let s={...previous,devices:[...(previous.devices||[])],windows:[...(previous.windows||[])]};
  if(key==='Session'){return {devices:[],windows:[],at:Date.now()};}
  if(key==='Progress'){const p=value.match(/^(\d+)(?:\/(\d+))?/);s.done=p?.[1]??null;s.total=p?.[2]??null;s.at=Date.now();}
  else if(key==='Rejected')s.rejected=value.match(/^\d+/)?.[0]??null;
  else if(key==='Restore.Point'){const p=value.match(/^(\d+)(?:\/(\d+))?/);s.restore=p?.[1]??null;s.restoreTotal=p?.[2]??null;}
  else if(key==='Recovered')s.recovered=value.match(/^\d+\/\d+/)?.[0]?.replace('/',' / ');
  else if(key==='Guess.Base'||key==='Guess.Mask')s.base=value;
  else if(key==='Guess.Mod')s.mod=value;
  else if(key.startsWith('Guess.Queue'))s[key==='Guess.Queue.Mod'?'modQueue':'queue']=value;
  else if(key==='Time.Estimated')s.etaText=value;
  else if(key==='Status')s.engineState=value;
  else if(key.startsWith('Restore.Sub'))s.sub=value;
  else if(key.startsWith('Candidates.#')){const id=Number(key.split('#')[1]);s.windows=s.windows.filter(w=>w.id!==id);s.windows.push({id,value});}
  else if(key.startsWith('Speed.#')){const p=value.match(/^([\d.]+)\s*([kMGTPE]?)H\/s/);if(!p)return null;const id=key.split('#')[1];if(id==='*')s.speed=Number(p[1])*1000**(p[2]?'kMGTPE'.indexOf(p[2])+1:0);else {s.devices=s.devices.filter(d=>d.device_id!==Number(id));s.devices.push({device_id:Number(id),speed:Number(p[1])*1000**(p[2]?'kMGTPE'.indexOf(p[2])+1:0)});}}
  else if(key.startsWith('Hardware.Mon.#')){const id=Number(key.split('#')[1]),d=s.devices.find(d=>d.device_id===id)||{device_id:id};s.devices=s.devices.filter(x=>x.device_id!==id);d.temp=Number(value.match(/Temp:\s*(\d+)/)?.[1]??-1);d.util=Number(value.match(/Util:\s*(\d+)/)?.[1]??-1);s.devices.push(d);}
  else return null;return s;
 }
 const percent=s=>s?.done!=null&&s?.total!=null&&BigInt(s.total)>0n?Number(BigInt(s.done)*10000n/BigInt(s.total))/100:null;
 const count=v=>integer(v)!==null?BigInt(v).toLocaleString(): '—';
 return {names,fileName,validate,argv,powershell,posix,command,decodeHex,size,restorePath,rebase,relocate,charsetConfig,patterns,readStatus,percent,count};
})();
