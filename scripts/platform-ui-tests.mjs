import {Window} from '../tests/browser/node_modules/happy-dom/lib/index.js';
import fs from 'node:fs';
import assert from 'node:assert/strict';

for(const platform of ['macos','linux']){
 const w=new Window({url:'https://hashhelm.test'}),messages=[];
 w.ipc={postMessage:s=>messages.push(JSON.parse(s))};w.HTMLElement.prototype.scrollIntoView=function(){};w.scrollTo=()=>{};w.confirm=()=>true;w.TextDecoder=TextDecoder;
 w.document.write(fs.readFileSync('ui/index.html','utf8').replace(/<script>[\s\S]*?<\/script>/,''));
 for(const file of ['core','i18n','app'])w.eval(fs.readFileSync('ui/'+file+'.js','utf8'));
 const $=id=>w.document.getElementById(id),last=op=>messages.filter(m=>m.op===op).at(-1);
 const resources=platform==='macos'?'/Applications/HashHelm.app/Contents/Resources':'/opt/HashHelm';
 const data=platform==='macos'?'/Users/Jenny/Library/Application Support/DigiForensics/HashHelm':'/home/jenny/.local/share/digiforensics/hashhelm';
 const engine=resources+'/hashcat/hashcat',dictionary=resources+'/assets/wordlists/chinese-common-10000.txt';
 w.receive({type:'init',platform,arch:'aarch64',buildLabel:'HashHelm 0.6.0 / '+platform+' arm64',resources,data,engine,engineExists:true,config:null,dictionaries:[{id:'chinese10k',label:'中文常见密码 · 1 万条',path:dictionary,count:10000}]});
 w.receive({type:'demo',target:data+'/demo.hash',wordlist:data+'/demo.dict'});
 w.document.querySelector('[data-kind=none]').click();$('wizardNext').click();
 assert.ok($('preview').textContent.includes("'"+engine+"'"));assert.ok(!$('preview').textContent.includes("& '"));assert.ok($('preview').textContent.includes(resources+'/assets/rules/light.rule'));
 $('wizardNext').click();assert.equal(last('run').engine,engine);assert.ok(last('run').args.includes(data+'/recovered.potfile'));assert.ok(!last('run').args.some(x=>x.endsWith('.exe')));
 assert.equal(last('save').config.resources,resources);assert.equal(last('save').config.data,data);
 w.I18n.setPreferences({language:'en',theme:'dark'});w.I18n.apply();assert.equal($('copyCommand').textContent,'Copy terminal command');
 w.receive({type:'finished',code:2,stopped:true});w.receive({type:'results',id:last('results').id,entries:[]});
 await w.happyDOM.close();
}
console.log('macOS/Linux frontend: independent data/resources, rule paths, POSIX previews, literal argv and preferences passed.');
