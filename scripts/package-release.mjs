// Stage a complete portable release only after verifying its embedded assets.
import {readFileSync, readdirSync, existsSync, mkdirSync, copyFileSync, cpSync, writeFileSync} from 'node:fs';
import {resolve, join, dirname, relative} from 'node:path';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import assert from 'node:assert/strict';

const exe=resolve(process.argv[2]??'target/release/satori.exe');
const stage=resolve(process.argv[3]??'release/satori');
const version=JSON.parse(readFileSync('package.json','utf8')).version;
assert(!existsSync(stage),'Output directory already exists; choose a fresh staging directory');
const verified=spawnSync(process.execPath,['scripts/verify-release.mjs',exe],{encoding:'utf8'});
assert.equal(verified.status,0,verified.stderr||verified.stdout);
const binary=readFileSync(exe);
assert.equal(binary.toString('ascii',0,2),'MZ');
assert.equal(binary.readUInt16LE(binary.readUInt32LE(0x3c)+4),0x8664,'Only Windows x64 is packaged here');
assert(binary.includes(Buffer.from(version,'utf16le')),'EXE version does not match package.json');
for(const script of ['ai.ps1','desktop-input.ps1']){
  assert(binary.includes(readFileSync(join('src-tauri/scripts',script))),`EXE has a stale ${script}`);
}
const hash=data=>createHash('sha256').update(data).digest('hex');
const build=join(dirname(exe),'build');
const loaders=readdirSync(build).filter(n=>n.startsWith('webview2-com-sys-'))
  .map(n=>join(build,n,'out/x64/WebView2Loader.dll')).filter(existsSync);
assert(loaders.length,'No x64 WebView2 loader was found');
assert.equal(new Set(loaders.map(n=>hash(readFileSync(n)))).size,1,'Conflicting cached loaders; rebuild in a clean target directory');
mkdirSync(stage,{recursive:true});
copyFileSync(exe,join(stage,'satori.exe'));
copyFileSync(loaders[0],join(stage,'WebView2Loader.dll'));
for(const file of ['LICENSE','PRIVACY.md'])copyFileSync(file,join(stage,file));
for(const folder of ['docs','browser-extension','third-party']){
  cpSync(folder,join(stage,folder),{recursive:true,filter:path=>!['native-host.json','quick-words-live.json'].includes(path.split(/[\\/]/).pop())});
}
for(const [file,argument] of [['Start-Real.cmd',''],['Start-Demo.cmd',' --demo'],['Start-Background.cmd',' --background']]){
  writeFileSync(join(stage,file),`@echo off\r\ncd /d "%~dp0"\r\nstart "" "%~dp0satori.exe"${argument}\r\n`);
}
writeFileSync(join(stage,'START-HERE.md'),`# Satori v${version} Alpha\n\n完整解压，先从托盘退出旧实例，再双击 satori.exe。保持 WebView2Loader.dll 同目录；系统需要 WebView2 Runtime。无需安装 Rust 或 Node。\n\nStart-Real.cmd 打开面板，Start-Background.cmd 后台启动，Start-Demo.cmd 进入演示。应用标识和数据库格式保持兼容，无需清空已有学习数据。\n\n这是未签名便携程序。具体构建检查和真机验收范围见 docs/BUILD-INFO.md；本目录 BUILD-INFO.md 记录实际包内 EXE 校验值。\n`);
writeFileSync(join(stage,'BUILD-INFO.md'),`# Satori v${version} · 当前便携包\n\nEXE SHA-256：${hash(binary)}\n\n本包由 scripts/package-release.mjs 在验证当前生产 HTML、JS、CSS、图标、版本和辅助脚本后生成。包含浏览器扩展、启动脚本和许可；SHA256SUMS.txt 核对所有包内文件。\n\n源码随附的 docs/BUILD-INFO.md 是交付构建的详细记录。若在 CI 或其他机器重新构建，EXE 哈希会不同，以本文件为准；编译和静态资源验证不代表所有 Windows 硬件已经验收。\n`);
function files(folder){return readdirSync(folder,{withFileTypes:true}).flatMap(e=>e.isDirectory()?files(join(folder,e.name)):[join(folder,e.name)]);}
writeFileSync(join(stage,'SHA256SUMS.txt'),files(stage).sort().map(file=>`${hash(readFileSync(file))}  ${relative(stage,file).replaceAll('\\','/')}\n`).join(''));
console.log(`Staged complete Satori v${version}: ${stage}`);
