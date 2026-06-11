#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct LixSession LixSession;

typedef struct CPtrResult_c_char {
  bool is_success;
  char *data;
} CPtrResult_c_char;

typedef void (*CActionCompleteFn_u32)(uint32_t);

typedef struct Test1Value {
  uint32_t value1;
  bool value2;
} Test1Value;

uint32_t lix_version(void);

struct LixSession *open(void);

bool close(struct LixSession *ptr);

struct CPtrResult_c_char get_active_branch(struct LixSession *ptr);

uintptr_t get_active_branch_buffer(struct LixSession *ptr, uint8_t *out_buf, uintptr_t buf_len);

bool free_active_branch(char *ptr);

uintptr_t Test1(struct LixSession *ptr, uint8_t *out_buf, uintptr_t buf_len, uintptr_t limit);

bool Test2Async(struct LixSession *ptr,
                uint8_t *out_buf,
                uintptr_t buf_len,
                CActionCompleteFn_u32 callback);
