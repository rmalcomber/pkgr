const child = new Deno.Command("ping.exe", {
  args: ["localhost", "-t"],

  detached: true,

  stdin: "inherit",
  stdout: "inherit",
  stderr: "inherit",
}).spawn();

child.unref();

console.log("Parent finished");
