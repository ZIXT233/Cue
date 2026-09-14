// cmd.exe parses metacharacters before a batch shim forwards its arguments.
// Follow node-cross-spawn's cmd escaping (MIT), including npm .bin's extra pass.
const meta = /([()\][%!^"`<>&|;, *?])/g;

export function windowsCommand(file: string, args: string[], comspec = process.env.ComSpec || "cmd.exe") {
  if (!/\.(cmd|bat)$/i.test(file)) return { executable: file, args, windowsVerbatimArguments: false };
  const doubleEscape = /node_modules[\\/]\.bin[\\/][^\\/]+\.cmd$/i.test(file);
  const quotedArgs = args.map(value => {
    let escaped = '"' + value.replace(/(\\*)"/g, '$1$1\\"').replace(/(\\+)$/, '$1$1') + '"';
    escaped = escaped.replace(meta, '^$1');
    return doubleEscape ? escaped.replace(meta, '^$1') : escaped;
  });
  const command = [file.replaceAll('/', '\\').replace(meta, '^$1'), ...quotedArgs].join(' ');
  return { executable: comspec, args: ['/d', '/s', '/c', `"${command}"`], windowsVerbatimArguments: true };
}
