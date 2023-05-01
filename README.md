# confidentail-quark-benchmart


## Config the hardware

### 1.1 Enable userspace governor because `intel_pstate` only has two governors available: powersave and performance:
+ disable the current driver: add intel_pstate=disable to your kernel boot line
+ boot, then load the userspace module: modprobe cpufreq_userspace
+ set the governor: cpupower frequency-set --governor userspace
+ set the frequency: cpupower --cpu all frequency-set --freq 800MHz

### 1.2 Set CUP frequency
set the maximum frequency for all cores:

```
cpupower frequency-set -u 3400mhz
```

set minimum frequency you can use
```
cpupower frequency-set -d 3400mhz
```
