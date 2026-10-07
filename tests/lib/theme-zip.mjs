import zlib from 'node:zlib';

/**
 * 生成一个「仅存储、不压缩」的 zip，用于在测试里构造主题包。
 * 只用到未压缩条目，因此不需要引入压缩库。
 */
export function makeZip(files){
 const crc32=zlib.crc32;
 const localParts=[],central=[];
 let offset=0;
 for(const [name,content] of Object.entries(files)){
  const data=Buffer.from(content,'utf8');
  const nameBuf=Buffer.from(name,'utf8');
  const crc=crc32(data)>>>0;
  const local=Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50,0);local.writeUInt16LE(20,4);local.writeUInt16LE(0,6);
  local.writeUInt16LE(0,8);local.writeUInt16LE(0,10);local.writeUInt16LE(0,12);
  local.writeUInt32LE(crc,14);local.writeUInt32LE(data.length,18);local.writeUInt32LE(data.length,22);
  local.writeUInt16LE(nameBuf.length,26);local.writeUInt16LE(0,28);
  localParts.push(local,nameBuf,data);
  const dir=Buffer.alloc(46);
  dir.writeUInt32LE(0x02014b50,0);dir.writeUInt16LE(20,4);dir.writeUInt16LE(20,6);
  dir.writeUInt16LE(0,8);dir.writeUInt16LE(0,10);dir.writeUInt16LE(0,12);dir.writeUInt16LE(0,14);
  dir.writeUInt32LE(crc,16);dir.writeUInt32LE(data.length,20);dir.writeUInt32LE(data.length,24);
  dir.writeUInt16LE(nameBuf.length,28);dir.writeUInt16LE(0,30);dir.writeUInt16LE(0,32);
  dir.writeUInt16LE(0,34);dir.writeUInt16LE(0,36);dir.writeUInt32LE(0,38);dir.writeUInt32LE(offset,42);
  central.push(dir,nameBuf);
  offset+=local.length+nameBuf.length+data.length;
 }
 const centralBuf=Buffer.concat(central);
 const end=Buffer.alloc(22);
 end.writeUInt32LE(0x06054b50,0);end.writeUInt16LE(0,4);end.writeUInt16LE(0,6);
 end.writeUInt16LE(Object.keys(files).length,8);end.writeUInt16LE(Object.keys(files).length,10);
 end.writeUInt32LE(centralBuf.length,12);end.writeUInt32LE(offset,16);end.writeUInt16LE(0,20);
 return Buffer.concat([...localParts,centralBuf,end]);
}

