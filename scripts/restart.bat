@ECHO OFF
taskkill /im proxy.exe /f
ping -n 1 127.1 >nul

%1 start mshta vbscript:createobject("wscript.shell").run("""%~0"" ::",0)(window.close)&&exit
start /b proxy.exe
