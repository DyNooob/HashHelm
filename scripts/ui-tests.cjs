
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
const ctx={window:{},TextDecoder,Uint8Array};vm.createContext(ctx);vm.runInContext(fs.readFileSync('ui/core.js','utf8'),ctx);const H=ctx.window.HC;
const c={engine:'D:/有 空格/hashcat.exe',target:'D:/哈希 文件/test.hash',hashMode:'0',session:'fh-test-1',attack:'0',wordlist:'D:/词典/test.txt',wordlist2:'',mask:'?d?d',customCharset:'',increment:false,incrementMin:'1',rules:'',pcfg:'',runtime:'60',devices:'',outfile:'',recipe:'',optimized:false};
const argv=()=>Array.from(H.argv(c,'D:/test workspace'));
assert.deepEqual(argv().slice(-2),[c.target,c.wordlist]);assert.equal(argv().filter(x=>x==='--runtime').length,1);
for(const [m,tail]of [['3',[c.target,c.mask]],['7',[c.target,c.mask,c.wordlist]],['6',[c.target,c.wordlist,c.mask]],['12',[c.target,c.mask,c.wordlist]]]){c.attack=m;assert.deepEqual(argv().slice(-tail.length),tail);}
c.attack='4';assert.ok(!argv().includes('-S'));c.slowCandidates=true;assert.ok(argv().includes('-S'));c.slowCandidates=false;assert.equal(argv().at(-1),c.target);assert.ok(!argv().includes(c.wordlist));c.runtime='';assert.throws(argv);c.runtime='60';c.attack='0';
c.devices='1; whoami';assert.throws(argv);c.devices='1,2';assert.ok(argv().includes('1,2'));c.devices='';
c.session='../escape';assert.throws(argv);c.session='fh-test-1';c.hashMode='4000';assert.throws(argv);c.recipe='md5(md5(salt) . pass)';assert.ok(argv().includes(c.recipe));c.hashMode='0';assert.ok(!argv().includes('--hash-recipe'));
c.attack='0';c.customCharset='?l';c.increment=true;assert.ok(!argv().includes('--increment'));assert.ok(!argv().includes('-1'));
assert.equal(H.size('?1?1?1?1','?l?u'),'7311616');assert.equal(H.size('?1?1','?d','1'),'110');assert.equal(H.size('?a?a?a?a'),'81450625');assert.equal(H.size('?q'),null);
assert.equal(H.decodeHex('e78e8be5b08fe6988e'), '王小明');assert.equal(H.decodeHex('3a2428486578293f'),':$(Hex)?');assert.equal(H.decodeHex('ff'),null);assert.equal(H.decodeHex(''), '');
assert.equal(H.restorePath(argv()),'D:/test workspace/fh-test-1.restore');assert.equal(H.restorePath(['--help']),'');
assert.equal(H.powershell("D:/O'Brien/hc.exe",["$(whoami)",'?d!']),"& 'D:/O''Brien/hc.exe' '$(whoami)' '?d!'");
console.log('Core: command order, literal arguments, mode-specific flags, runtime, recipe, exact keyspaces, JSON/hex results and PowerShell quoting passed.');


const moved=H.rebase({args:['C:/Old/HashHelm/workspace/a.hash','D:/external/file.zip'],engine:'C:\\Old\\HashHelm\\hashcat\\hashcat.exe'},'C:/Old/HashHelm/workspace','D:/Tools/HashHelm/workspace');
assert.equal(moved.args[0],'D:/Tools/HashHelm/workspace/a.hash');assert.equal(moved.args[1],'D:/external/file.zip');assert.equal(moved.engine,'D:/Tools/HashHelm/hashcat/hashcat.exe');
assert.equal(H.rebase('C:/Old/HashHelm2/keep','C:/Old/HashHelm/workspace','D:/Tools/HashHelm/workspace'),'C:/Old/HashHelm2/keep');
console.log('Portable paths: internal files rebased; external sources and similar prefixes preserved.');

const advanced={...c,attack:'3',mask:'?1?2?8',customCharset:'?l?u',customCharset2:'?d',customCharset8:'!?',devices:'1,2',deviceTypes:'1,2',kernelThreads:'64',kernelAccel:'128',kernelLoops:'256',virtualMulti:'2',statusTimer:'2',increment:false,markovDisable:true,rules:'',runtime:'3'};
// A literal question mark in a charset is escaped in Hashcat syntax.
advanced.customCharset8='!??';const aa=Array.from(H.argv(advanced,'D:/data'));
for(const [flag,value]of [['-d','1,2'],['-D','1,2'],['-T','64'],['-n','128'],['-u','256'],['-Y','2'],['-2','?d'],['-8','!??']])assert.equal(aa[aa.indexOf(flag)+1],value);
assert.ok(aa.includes('--markov-disable'));assert.ok(!aa.includes('-w'));assert.ok(!aa.includes('--status-json'));
assert.throws(()=>H.argv({...advanced,kernelThreads:'0'},'D:/data'));assert.throws(()=>H.argv({...advanced,customCharset2:'?2'},'D:/data'));assert.throws(()=>H.argv({...advanced,customCharset8:''},'D:/data'));
assert.equal(H.size('?1?2?8',H.charsetConfig(advanced)),'1040');
const patterns=Array.from(H.patterns({min:1,max:2,charset:'01',prefix:'CTF?',suffix:'!'}));assert.deepEqual(patterns.map(p=>p.mask),['CTF???1!','CTF???1?1!']);assert.ok(patterns.every(p=>!p.increment));assert.deepEqual(patterns.map(p=>H.size(p.mask,p.customCharset)),['2','4']);
const chained=Array.from(H.argv({...c,attack:'0',rules:'one.rule\ntwo.rule',rulesConcat:true},'D:/data'));assert.equal(chained.filter(x=>x==='-r').length,2);assert.ok(chained.includes('--rules-concat'));
for(const attack of ['1','3','4','6','7','9','12']){
 const args=Array.from(H.argv({...c,attack,wordlist2:'second.txt',rules:'one.rule\ntwo.rule'},'D:/data'));
 assert.equal(args.filter(x=>x==='-r').length,2,`Hashcat 8 rules must be available in mode ${attack}`);
}
const association={...c,attack:'9',wordlist:'',runtime:'60'};
assert.equal(Array.from(H.argv(association,'D:/data')).at(-1),c.target);
assert.ok(!Array.from(H.argv(association,'D:/data')).includes(''));
assert.throws(()=>H.argv({...association,runtime:''},'D:/data'));
assert.deepEqual(Array.from(H.argv({...association,wordlist:'account-words.txt'},'D:/data')).slice(-2),[c.target,'account-words.txt']);
assert.equal(H.strategyKey(c),H.strategyKey(Object.fromEntries(Object.entries({...c,session:'another-session',count:20}).reverse())));
assert.notEqual(H.strategyKey(c),H.strategyKey({...c,rules:'other.rule'}));
assert.notEqual(H.strategyKey(c),H.strategyKey({...c,runtime:'120'}));
console.log('Advanced: native multi-device flags, autotune overrides, eight charsets, rules concat, fixed prefix/suffix lengths and validation passed.');

let status={};for(const line of fs.readFileSync('tests/fixtures/hashcat/status-v8-mask.txt','utf8').split(/\r?\n/))status=H.readStatus(line,status)||status;
assert.equal(status.done,'24964497408');assert.equal(status.total,'6634204312890625');assert.equal(status.windows[0].id,1);assert.ok(status.windows[0].value.includes(' -> '));assert.equal(status.devices[0].speed,8645100000);assert.equal(status.restoreTotal,'7737809375');
const large=H.readStatus('{"progress":[9007199254740993,18446744073709551615],"rejected":23,"restore_point":9007199254740995,"guess":{"guess_base":"D:/词典/123.txt","guess_base_offset":1,"guess_base_count":2},"devices":[]}');
assert.equal(large.done,'9007199254740993');assert.equal(large.total,'18446744073709551615');assert.equal(large.restore,'9007199254740995');assert.equal(large.base,'D:/词典/123.txt');assert.equal(H.count(large.done),'9,007,199,254,740,993');
assert.equal(H.percent(H.readStatus('Progress.........: 123')),null);assert.equal(H.readStatus('Progress.........: [Protected]').done,null);assert.equal(H.readStatus('not a status'),null);assert.equal(H.readStatus('Speed.#01........: 12 H/s').devices[0].speed,12);
assert.deepEqual(Array.from(H.readStatus('Session..........: next',status).windows),[]);
console.log('Telemetry: real Hashcat fixture, exact uint64 JSON counters, candidate windows, hardware metrics, unknown totals and frame resets passed.');

assert.equal(H.command("/opt/O'Brien/hashcat",['$(whoami)','?d!','a b'],'linux'),"'/opt/O'\"'\"'Brien/hashcat' '$(whoami)' '?d!' 'a b'");
assert.equal(H.command('D:/hc.exe',['--help'],'windows'),H.powershell('D:/hc.exe',['--help']));
const unix=H.relocate({engine:'/old/app/hashcat/hashcat',args:['/old/data/file.hash','/external/words.txt','/Old/app/keep']},{oldData:'/old/data',newData:'/Users/Jenny/Library/Application Support/HashHelm',oldResources:'/old/app',newResources:'/Applications/HashHelm.app/Contents/Resources'});
assert.equal(unix.engine,'/Applications/HashHelm.app/Contents/Resources/hashcat/hashcat');assert.equal(unix.args[0],'/Users/Jenny/Library/Application Support/HashHelm/file.hash');assert.equal(unix.args[1],'/external/words.txt');assert.equal(unix.args[2],'/Old/app/keep');
console.log('Cross-platform: POSIX shell quoting, separate resources/data relocation, external paths and case sensitivity passed.');

const stats={words:{'one.txt':{count:'10'},'two.txt':{count:'20'}},rules:{'a.rule':{count:'3'},'b.rule':{count:'5'}}};
const weighted={...c,wordlist:'one.txt',wordlist2:'two.txt',rules:'a.rule\nb.rule',rulesConcat:false,increment:false};
assert.equal(H.workload({...weighted,attack:'0'},stats).candidates,'150');
assert.equal(H.workload({...weighted,attack:'0',rulesConcat:true},stats).candidates,'80');
assert.equal(H.workload({...weighted,attack:'1'},stats).candidates,'3000');
assert.equal(H.workload({...weighted,attack:'6',mask:'?d?d'},stats).candidates,'15000');
assert.equal(H.workload({...weighted,attack:'3',rules:'',mask:'?1?1',customCharset:'?l?u'},stats).candidates,'2704');
assert.equal(H.workload({...weighted,attack:'3',rules:'',mask:'?d'.repeat(20)},stats).candidates,'100000000000000000000');
assert.equal(H.workload({...weighted,attack:'9'},stats).candidates,null);
assert.equal(H.workload({...weighted,attack:'0',rules:'missing.rule'},stats).candidates,null);
assert.equal(H.workload({...weighted,attack:'0',rules:''},{words:{'one.txt':{count:null,reason:'missing'}},rules:{}},100).candidates,null);
assert.equal(H.duration(null),'待测速');assert.equal(H.duration(0.1),'< 1 秒');assert.equal(H.duration(3601),'2 小时');
assert.notEqual(H.speedKey(c),H.speedKey({...c,optimized:!c.optimized}));assert.notEqual(H.speedKey(c),H.speedKey({...c,devices:'2'}));
console.log('Workload: exact bigint masks, combination/hybrid multiplication, concatenated vs chained rules, unknown contexts and benchmark configuration separation passed.');
