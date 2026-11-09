
import {Window} from '../tests/browser/node_modules/happy-dom/lib/index.js';
import fs from 'node:fs';import assert from 'node:assert/strict';
const w=new Window({url:'https://hashhelm.test'}),messages=[];
w.ipc={postMessage:s=>messages.push(JSON.parse(s))};w.HTMLElement.prototype.scrollIntoView=function(){};w.scrollTo=()=>{};w.confirm=()=>true;w.TextDecoder=TextDecoder;
const html=fs.readFileSync('ui/index.html','utf8').replace('<!-- LICENSE -->',fs.readFileSync('LICENSE','utf8').replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;'));w.document.write(html.replace(/<script>[\s\S]*?<\/script>/,'').replace('/* STYLES */',fs.readFileSync('ui/style.css','utf8')));w.eval(fs.readFileSync('ui/core.js','utf8'));w.eval(fs.readFileSync('ui/i18n.js','utf8'));w.eval(fs.readFileSync('ui/app.js','utf8'));
const $=id=>w.document.getElementById(id),click=id=>$(id).click(),last=op=>messages.filter(m=>m.op===op).at(-1),wait=()=>new Promise(r=>setTimeout(r,130));
const dictionaries=[['chinese10k','zh.txt',10000],['common10k','common.txt',10001],['chinese100k','large.txt',100000],['hanzi','hanzi.txt',7776],['pinyin','pinyin.txt',7744]].map(([id,f,count])=>({id,label:id,count,path:'D:/app/assets/wordlists/'+f}));
w.receive({type:'init',engine:'D:/app/hashcat/hashcat.exe',engineExists:true,data:'D:/app/workspace',config:null,dictionaries});
assert.equal($('environmentText').textContent,'引擎文件已找到');click('homeRecover');assert.equal(last('pick').field,'target');
w.receive({type:'picked',field:'target',path:'D:/有 空格/photos.zip'});const imp=last('import');assert.equal($('wizardNext').disabled,true);
w.receive({type:'imported',id:imp.id-1,source:'stale',hashPath:'stale',format:'ZIP',members:[]});assert.equal($('target').value,'');
w.receive({type:'imported',id:imp.id,source:imp.path,hashPath:'D:/app/workspace/target.hash',format:'ZIP · ZipCrypto',mode:17200,note:'read-only',members:[{index:0,name:'<img src=x>',size:12}],selected:0});
assert.equal($('zipMember').querySelector('img'),null);assert.equal($('hashMode').value,'17200');click('wizardNext');w.document.querySelector('[data-kind=profile]').click();click('fillJenny');click('wizardNext');
const pr=last('profile');assert.equal(pr.profile.names,'Jenny');assert.equal($('wizardNext').disabled,true);
w.receive({type:'profile_done',id:pr.id-1,generation:{path:'bad'}});assert.equal($('wizardNext').disabled,true);
w.receive({type:'profile_done',id:pr.id,generation:{count:2281,path:'D:/app/workspace/profiles/candidates.txt',preview:['Jenny','Jenny20000103!'],truncated:false}});
assert.equal($('stepTitle').textContent,'检查本次恢复计划');assert.equal($('attackPlan').children.length,4);
click('wizardNext');let run=last('run');assert.ok($('preview').textContent.includes(w.HC.powershell(run.engine,run.args)));assert.ok(run.args.includes('--runtime'));assert.equal(run.args[run.args.indexOf('--runtime')+1],'60');assert.ok(run.args.includes('D:/app/workspace/profiles/candidates.txt'));assert.equal(last('save').config.groups.length,1);
w.receive({type:'started',logPath:'D:/app/workspace/runs/1.log'});w.receive({type:'lines',lines:[JSON.stringify({progress:[5,10],recovered_hashes:[0,1],devices:[{device_id:1,speed:1200000,util:92,temp:64}]})]});
assert.equal($('percent').textContent,'50.00%');assert.equal($('speed').textContent,'1.20 MH/s');assert.ok($('deviceMetrics').textContent.includes('64°C'));
w.receive({type:'finished',code:4,stopped:false});await wait();assert.equal(messages.filter(m=>m.op==='run').length,2);
run=last('run');assert.ok(run.args.includes('D:/app/assets/wordlists/zh.txt'));assert.ok(!run.args.includes('D:/app/workspace/profiles/candidates.txt'));
w.receive({type:'finished',code:0,stopped:false});const result=last('results');assert.ok(result);assert.equal(messages.filter(m=>m.op==='run').length,2);
const g=last('save').config.groups[0];assert.equal(g.stages[0].state,'limited');assert.equal(g.stages[1].state,'found');assert.equal(g.stages[2].state,'skipped');assert.equal(g.state,'found');
w.receive({type:'results',id:result.id-1,entries:[{password_hex:'626164'}]});assert.ok(!$('resultEntries').textContent.includes('bad'));
w.receive({type:'results',id:result.id,entries:[{hash_hex:'68617368',password_hex:Buffer.from('王小明:abc?').toString('hex')}]});assert.ok($('resultEntries').textContent.includes('王小明:abc?'));assert.equal($('resultEntries').querySelector('button').textContent,'复制密码');
w.document.querySelector('[data-page=devices]').click();assert.ok(!$('page-monitor').classList.contains('active'));assert.ok($('page-devices').classList.contains('active'));
click('homeRecover');w.receive({type:'picked',field:'target',path:'D:/four.hash'});const im2=last('import');w.receive({type:'imported',id:im2.id,source:im2.path,hashPath:'four.hash',format:'哈希文本',mode:0,note:'raw',members:[]});
assert.equal($('hashMode').value,'');click('wizardNext');assert.ok($('flowError').textContent.includes('算法'));$('rawHashMode').value='0';$('rawHashMode').dispatchEvent(new w.Event('input'));click('wizardNext');
w.document.querySelector('[data-kind=pattern]').click();click('letters4');$('planBreadth').value='only';click('wizardNext');assert.equal($('attackPlan').children.length,1);assert.ok($('searchScope').textContent.includes('7,311,616'));
click('wizardNext');run=last('run');assert.ok(run.args.includes('?l?u'));assert.ok(!run.args.includes('--increment'));assert.ok(!run.args.includes('-r'));
w.receive({type:'finished',code:1,stopped:false});w.receive({type:'results',id:last('results').id,entries:[]});assert.equal($('resultTitle').textContent,'尚未恢复出密码');
$('rawArgs').value='["--help","$(whoami)"]';click('rawRun');assert.deepEqual(last('run').args,['--help','$(whoami)']);w.receive({type:'finished',code:0,stopped:false});
console.log('Workflow: import/stale replies/XSS, raw algorithm confirmation, profile generation, stable plan commands, single grouped history, timed-stage advance, early stop, isolated JSON results, device metrics, exact masks and raw argv passed.');

w.I18n.setPreferences({language:'en',theme:'dark'},true);w.I18n.apply();
assert.equal(w.I18n.t('2,281 个候选 · 每阶段最多 60 秒'),'2,281 candidates · up to 60 seconds per stage');assert.equal(w.document.documentElement.lang,'en');assert.equal(w.document.documentElement.dataset.theme,'dark');
assert.equal($('wizardNext').textContent,'Start recovery →');assert.ok($('resultTitle').textContent.includes('No password'));
assert.equal(last('save').config.preferences.language,'en');assert.equal(last('save').config.preferences.theme,'dark');
const untranslated=[];const walk=w.document.createTreeWalker(w.document.body,w.NodeFilter.SHOW_TEXT);let item;
while(item=walk.nextNode()){if(item.parentElement.closest('script,style,svg,pre,code,textarea,#profilePreview,#fileName,#filePillName,#reviewTarget,#monitorTitle,#monitorSource,.recent-row strong,.recent-row small,.history-info h2,.history-info p,.result-hash,#zipMember'))continue;if(/[\u4e00-\u9fff]/.test(item.nodeValue))untranslated.push(item.nodeValue.trim());}
assert.deepEqual([...new Set(untranslated)].filter(v=>v!=='简体中文'),[]);console.log('English UI inventory: no untranslated interface text.');
w.I18n.setPreferences({language:'zh-CN',theme:'light'},true);w.I18n.apply();assert.equal($('wizardNext').textContent,'开始恢复 →');assert.equal(w.document.documentElement.dataset.theme,'light');
click('probe');assert.equal($('stopAux').classList.contains('hidden'),false);click('stopAux');assert.equal(last('stop').op,'stop');w.receive({type:'finished',code:2,stopped:true});assert.equal($('stopAux').classList.contains('hidden'),true);
console.log('Appearance: English/Chinese live switching and saved light/dark preferences passed.');

// User-configured stage sequence, rather than a hard-coded escalation strategy.
click('homeRecover');w.receive({type:'picked',field:'target',path:'D:/mixed.zip'});const im3=last('import');w.receive({type:'imported',id:im3.id,source:im3.path,hashPath:'mixed.hash',format:'ZIP',mode:17200,note:'read-only',members:[]});click('wizardNext');w.document.querySelector('[data-kind=pattern]').click();
$('lengthMin').value='1';$('lengthMax').value='2';$('enumerationCharset').value='01';$('knownPrefix').value='CTF?';$('knownSuffix').value='!';$('planBreadth').value='only';$('devices').value='1,2';$('deviceTypes').value='2';$('kernelThreads').value='64';click('wizardNext');assert.equal($('attackPlan').children.length,2);
click('appendStage');w.document.querySelector('[data-kind=known]').click();$('knownCandidates').value='Jenny!';click('wizardNext');w.receive({type:'candidates',path:'D:/app/workspace/candidates-extra.txt'});assert.equal($('attackPlan').children.length,3);
click('wizardNext');run=last('run');assert.equal(run.args.at(-1),'CTF???1!');assert.equal(run.args[run.args.indexOf('-d')+1],'1,2');assert.equal(run.args[run.args.indexOf('-T')+1],'64');
w.document.querySelector('[data-page=help]').click(); // Telemetry continues while another page is open.
w.receive({type:'lines',lines:['Session..........: mixed','Status...........: Running','Guess.Base.......: File (words.txt)','Speed.#01........: 1234 H/s','Recovered........: 0/1 (0.00%)','Progress.........: 1/2 (50.00%)','Rejected.........: 0/1 (0.00%)','Restore.Point....: 0/2 (0.00%)','Candidates.#01...: CTF?0! -> CTF?1!']});
assert.equal($('checkedCount').textContent,'1');assert.ok($('candidateWindows').textContent.includes('CTF?0! -> CTF?1!'));assert.equal($('guessBase').textContent,'File (words.txt)');
for(let i=2;i<26;i++)w.receive({type:'lines',lines:['Session..........: mixed','Progress.........: '+i+'/100','Candidates.#01...: <img src=x> -> abc']});assert.equal($('candidateWindows').querySelector('img'),null);
w.receive({type:'finished',code:2,stopped:true,seconds:3});w.receive({type:'results',id:last('results').id,entries:[]});const mixed=last('save').config.groups[0];assert.equal(mixed.stages.length,3);assert.equal(mixed.stages[0].telemetry.done,'25');assert.ok(mixed.stages[0].snapshots.length<=20);
$('runStages').children[1].click();assert.equal($('checkedCount').textContent,'—');assert.ok(!$('candidateWindows').textContent.includes('<img'));$('runStages').children[0].click();assert.equal($('checkedCount').textContent,'25');click('exportProgress');assert.equal(last('export').kind,'progress');assert.ok(JSON.parse(last('export').text).stages[0].recentSnapshots.length);w.document.querySelector('[data-page=history]').click();[...$('historyList').querySelectorAll('button')].find(b=>b.textContent==='查看任务').click();assert.equal($('checkedCount').textContent,'25');w.receive({type:'results',id:last('results').id,entries:[]});
$('composerTarget').value='D:/case/hash file.txt';$('composerMode').value='0';$('composerAttack').value='6';$('composerDictionary').value='D:/words.txt';$('composerMask').value='?d?d';click('composeArgs');const composed=JSON.parse($('rawArgs').value);assert.deepEqual(composed.slice(-3),['D:/case/hash file.txt','D:/words.txt','?d?d']);assert.equal(last('run').args.at(-1),'CTF???1!');
w.I18n.setPreferences({language:'en',theme:'dark'});w.I18n.apply();assert.ok($('candidateWindows').textContent.includes('<img src=x>'));assert.equal($('checkedCount').textContent,'25');w.I18n.setPreferences({language:'zh-CN',theme:'light'});w.I18n.apply();
console.log('Advanced workflow: ordered mixed stages, fixed suffixes, real-status parsing off-page, bounded safe candidate snapshots, saved history and non-executing command composer passed.');
w.document.querySelector('[data-page=about]').click();assert.ok($('page-about').classList.contains('active'));assert.ok($('page-about').textContent.includes('Originally developed by DyNooob.'));assert.ok($('page-about').querySelector('.license-text').textContent.includes('HashHelm Attribution License'));
$('page-about').querySelector('[data-external=website]').click();assert.equal(last('open_external').link,'website');$('page-about').querySelector('[data-external=repository]').click();assert.equal(last('open_external').link,'repository');
w.I18n.setPreferences({language:'en',theme:'dark'});w.I18n.apply();assert.equal($('page-about').querySelector('h1').textContent,'About HashHelm');
console.log('Attribution: accessible About page, full license, intact author URLs and native-link IPC passed.');
await w.happyDOM.close();
