# example Module

## Admin Guide

### Overview

The module keeps a counter.

### Exported Parameters

#### counter_start (integer)

The first value of the counter.

```
modparam("example", "counter_start", 1)
# a comment inside the example
```

#### counter_name(string)

The name the counter is shown under.

### Exported Functions

#### count_up(step)

Adds step to the counter.
