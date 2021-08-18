@ECHO OFF

taskkill /im proxy.exe /f

ping -n 1 127.1 >nul