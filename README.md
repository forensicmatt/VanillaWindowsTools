[![Build and Test](https://github.com/forensicmatt/VanillaWindowsTools/actions/workflows/tests.yml/badge.svg?branch=main)](https://github.com/forensicmatt/VanillaWindowsTools/actions/workflows/tests.yml)

# VanillaWindowsTools
Tools for parsing and playing with https://github.com/AndrewRathbun/VanillaWindowsReference data

## Docker
You can pre-download and generate the index and pass volumes to the container. Example:
```
docker run -v G:\Temp\vanilla-data:/vanilla/data -v G:\Temp\index:/vanilla/index -p 8000:8000 vanillawindowstools:latest
```

## vanilla_service
This service will run a web service which exposes a REST api for lookups.

```
> target\release\vanilla_service.exe -i F:\testing\index -s .\VanillaWindowsReference
```

See `/examples.rest` for examples.

## vanilla_to_json
This tool will match up the SystemInfo file with the respected csv file listing and print jsonl representation.

This tool currently excludes the following csv fields (will work to make this configurable via the cli): 
 - LastAccessTimeUtc
 - LastWriteTimeUtc
 - Sddl
 - DirectoryName

**Example:**
```
H:\Dev\VanillaWindowsTools>target\release\vanillawinrefs_to_jsonl.exe -s VanillaWindowsReference\Windows10\1507\W10_1507_Pro_20150729_10240
{"Attributes":"Archive","CreationTimeUtc":"11/19/2021 8:59:24 PM","FullName":"C:\\PsExec_IgnoreThisFile_ResearchTool.exe","Length":"834936","MD5":"C590A84B8C72CF18F35AE166F815C9DF","Name":"PsExec_IgnoreThisFile_ResearchTool.exe","OsName":"Microsoft Windows 10 Pro","OsVersion":"10.0.10240 N/A Build 10240","SHA256":"57492D33B7C0755BB411B22D2DFDFDF088CBBFCD010E30DD8D425D5FE66ADFF4"}
{"Attributes":"Archive","CreationTimeUtc":"11/19/2021 8:59:42 PM","FullName":"C:\\test.csv","Length":"0","MD5":"","Name":"test.csv","OsName":"Microsoft Windows 10 Pro","OsVersion":"10.0.10240 N/A Build 10240","SHA256":""}
{"Attributes":"Archive","CreationTimeUtc":"7/10/2015 11:00:41 AM","FullName":"C:\\Program Files\\Common Files\\microsoft shared\\ink\\Alphabet.xml","Length":"791421","MD5":"6176656C4D6A215BD670D5BD63D35B59","Name":"Alphabet.xml","OsName":"Microsoft Windows 10 Pro","OsVersion":"10.0.10240 N/A Build 10240","SHA256":"E066F5907F9EFDB760DA0377A7B5664C815D667FB2A7B370AA4A49783F4FEA0D"}
```