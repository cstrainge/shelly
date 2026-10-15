
use std::{ fmt::{ self, Debug, Display, Formatter },sync::atomic::AtomicU64 };

use crate::language::data::{ exec_result::ExecResult, range::Range };



pub type TypeId = u16;


pub struct TypeError
{
    pub expected: TypeId,
    pub found: TypeId
}


impl Debug for TypeError
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        write!(f,
               "Type error expected type ID {}, but found type ID {}.",
               self.expected,
               self.found)
    }
}


pub type ValueResult<T> = Result<T, TypeError>;


/**
 * All heap-allocated values in the Shelly language have this header.
 */
#[repr(C)]
pub struct ValueHeapHeader
{
    /**
     * Reference count of the object. Must use atomic instructions to modify this field.
     */
    pub ref_count: AtomicU64,

    /**
     * Type identifier of the heap-allocated object.
     */
    pub type_id: TypeId,

    /**
     * Reserved for future use.
     */
    pub reserved: u16,

    /**
     * Length of the heap-allocated data that follows this header.
     */
    pub len: u32
}


#[repr(C)]
pub struct LongRange
{
    header: ValueHeapHeader,

    has_start: bool,
    has_end: bool,
    inclusive: bool,

    start: i64,
    end: i64
}


/**
 * Represents a value in the Shelly language, encapsulating its flags and storage information.
 *
 * ```text
 * word0
 *   0 --> Control state: 00      None
 *   1   |                01      PropagatingError
 *   2   |                02 - FF Reserved
 *   3   |
 *   4   |
 *   5   |
 *   6   |
 *   7 -->
 *   8 --> Value Flags -> Is Error
 *   9   |             -> Reserved(0)
 *  10   |             -> Reserved(0)
 *  11   |             -> Reserved(0)
 *  12   |             -> Reserved(0)
 *  13   |             -> Reserved(0)
 *  14   |             -> Reserved(0)
 *  15 -->             -> Reserved(0)
 *  16 --> -- System Type Id 0 - 65,535
 *  17   |
 *  18   |                   Embedded                      Heap allocated.
 *  19   |                   0000 None                     0010 String
 *  20   |                   0001 Bool                     0011 Array
 *  21   |                   0002 ExecResult               0012 HashMap
 *  22   |                   0003 Integer                  0013 Struct
 *  23   |                   0004 Float                    0014 TypeInfo
 *  24   |                   0005 Range (start/end)        0015 Callable
 *  25   |                   0006 Enum  (value/type)       0016 CallableCapture
 *  26   |                                                 0017 Union
 *  27   |                                                 0018 Large Range
 *  28   |                                                 0019 Any (Can be a boxed embedded value)
 *  29   |
 *  30   |
 *  31 -->
 *  32 -->-- Range Bits -> Has Start  -->-- If Hash, this is the value type id.
 *  33   |                 Has End      |
 *  34 -->                 Inclusive    |
 *  35                                  |
 *  36                                  |
 *  37                                  |
 *  38                                  |
 *  39                                  |
 *  40                                  |
 *  41                                  |
 *  42                                  |
 *  43                                  |
 *  44                                  |
 *  45                                  |
 *  46                                  |
 *  47 --------------------------------->
 *  48 -->-- Referenced Type Id
 *  49   |   If type is struct, then this field contains the referenced type id of the struct.
 *  50   |   If Enum, then the enum's referenced type id.
 *  51   |   If Union, then the union's referenced type id.
 *  52   |   If hash this is the key type id.
 *  52   |   If array this is the element type id.
 *  53   |   If callable then this is the id of the callable signature.
 *  54   |
 *  55   |
 *  56   |
 *  57   |
 *  58   |
 *  59   |
 *  60   |
 *  61   |
 *  62   |
 *  63 -->
 * ```
 */
#[repr(C)]
pub struct Value
{
    word0: u64,
    word1: u64
}


pub const TYPE_ID_NONE:       TypeId = 0x0;
pub const TYPE_ID_BOOL:       TypeId = 0x1;
pub const TYPE_ID_EXECRESULT: TypeId = 0x2;
pub const TYPE_ID_INTEGER:    TypeId = 0x3;
pub const TYPE_ID_FLOAT:      TypeId = 0x4;
pub const TYPE_ID_RANGE:      TypeId = 0x5;
pub const TYPE_ID_ENUM:       TypeId = 0x6;


const CONTROL_STATE_MAX: u64 = 0xFF;
const ERROR_FLAG_MASK: u64 = 1 << 8;


const ENUM_CONTROL_STATE_NORMAL: u64 = 0x0;
const ENUM_CONTROL_STATE_PROPAGATING_ERROR: u64 = 0x1;


const SYSTEM_TYPE_ID_SHIFT: u64 = 16;
const SYSTEM_TYPE_ID_MASK: u64 = 0xFFFF << SYSTEM_TYPE_ID_SHIFT;


const RANGE_BITS_SHIFT: u64 = 32;
const RANGE_BITS_MASK: u64 = 0x7 << RANGE_BITS_SHIFT;


impl Value
{
    pub fn new_none() -> Self
    {
        Self { word0: 0, word1: 0 }
    }

    pub fn new_bool(value: bool) -> Self
    {
        let mut new_self = Self { word0: 0 as u64, word1: value as u64 };

        new_self.set_type_id(TYPE_ID_BOOL);
        new_self
    }

    pub fn new_exec_result(value: ExecResult) -> Self
    {
        let raw_result: u64 = ((value.signaled as u64) << 8) | (value.code as u64);

        let mut new_self = Self { word0: 0 as u64, word1: raw_result };

        new_self.set_type_id(TYPE_ID_EXECRESULT);
        new_self
    }

    pub fn new_integer(value: i64) -> Self
    {
        let mut new_self = Self { word0: 0 as u64, word1: value as u64 };

        new_self.set_type_id(TYPE_ID_INTEGER);
        new_self
    }

    pub fn new_float(value: f64) -> Self
    {
        let mut new_self = Self { word0: 0 as u64, word1: value.to_bits() };

        new_self.set_type_id(TYPE_ID_FLOAT);
        new_self
    }

    pub fn new_range(range: Range) -> Self
    {
        let start = range.start.unwrap_or(0);
        let end = range.end.unwrap_or(0);

        // In the future convert to a heap allocated range representation if needed.

        let raw_range: u64 = ((start as u32 as u64) << 32) | (end as u32 as u64);
        let mut new_self = Self { word0: 0 as u64, word1: raw_range };

        new_self.set_type_id(TYPE_ID_RANGE);
        new_self.set_range_bits(range.start.is_some(),
                                range.end.is_some(),
                                range.inclusive);
        new_self
    }

    pub fn set_error(mut self) -> Self
    {
        self.word0 = (self.word0 & !ERROR_FLAG_MASK) | ((true as u64) << 8);
        self
    }

    pub fn clear_error(mut self) -> Self
    {
        self.word0 = self.word0 & !ERROR_FLAG_MASK;
        self
    }

    pub fn get_error(&self) -> bool
    {
        (self.word0 & ERROR_FLAG_MASK) != 0
    }

    pub fn set_error_propagation(mut self) -> Self
    {
        self.word0 =   (self.word0 & !CONTROL_STATE_MAX)
                     | ((ENUM_CONTROL_STATE_PROPAGATING_ERROR as u64));
        self
    }

    pub fn clear_error_propagation(mut self) -> Self
    {
        self.word0 = (self.word0 & !CONTROL_STATE_MAX)
                  | ((ENUM_CONTROL_STATE_NORMAL as u64));
        self
    }

    pub fn get_error_propagation(&self) -> bool
    {
        (self.word0 & CONTROL_STATE_MAX) == ENUM_CONTROL_STATE_PROPAGATING_ERROR
    }

    pub fn type_id(&self) -> TypeId
    {
        ((self.word0 & SYSTEM_TYPE_ID_MASK) >> SYSTEM_TYPE_ID_SHIFT) as TypeId
    }

    fn expect_type(&self, expected: TypeId) -> ValueResult<()>
    {
        let found = self.type_id();

        if found != expected
        {
            return Err(TypeError { expected, found });
        }

        Ok(())
    }

    pub fn as_bool(&self) -> ValueResult<bool>
    {
        self.expect_type(TYPE_ID_BOOL)?;

        Ok(self.word1 != 0)
    }

    pub fn as_exec_result(&self) -> ValueResult<ExecResult>
    {
        self.expect_type(TYPE_ID_EXECRESULT)?;

        Ok(ExecResult
            {
                signaled: ((self.word1 >> 8) & 1) != 0,
                code: (self.word1 & 0xFF) as _
            })
    }

    pub fn as_integer(&self) -> ValueResult<i64>
    {
        self.expect_type(TYPE_ID_INTEGER)?;

        Ok(self.word1 as i64)
    }

    pub fn as_float(&self) -> ValueResult<f64>
    {
        self.expect_type(TYPE_ID_FLOAT)?;

        Ok(f64::from_bits(self.word1))
    }

    pub fn as_range(&self) -> ValueResult<Range>
    {
        self.expect_type(TYPE_ID_RANGE)?;

        let bits = (self.word0 & RANGE_BITS_MASK) >> RANGE_BITS_SHIFT;

        let has_start = (bits & 0b001) != 0;
        let has_end = (bits & 0b010) != 0;
        let inclusive = (bits & 0b100) != 0;

        let start = (self.word1 >> 32) as u32 as i32;
        let end = self.word1 as u32 as i32;

        Ok(Range
            {
                start: if has_start { Some(start as _) } else { None },
                end: if has_end { Some(end as _) } else { None },
                inclusive
            })
    }

    fn set_type_id(&mut self, type_id: TypeId)
    {
        self.word0 =   (self.word0 & !SYSTEM_TYPE_ID_MASK)
                     | ((type_id as u64) << SYSTEM_TYPE_ID_SHIFT);
    }

    fn set_range_bits(&mut self, start_bit: bool, end_bit: bool, inclusive: bool)
    {
        self.word0 =   (self.word0 & !RANGE_BITS_MASK)
                     | (  ((start_bit as u64) << RANGE_BITS_SHIFT)
                        | ((end_bit as u64) << (RANGE_BITS_SHIFT + 1))
                        | ((inclusive as u64) << (RANGE_BITS_SHIFT + 2)));
    }
}


impl Display for Value
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        match self.type_id()
        {
            TYPE_ID_NONE       => write!(f, "()"),
            TYPE_ID_BOOL       => write!(f, "{}", self.as_bool().unwrap()),
            TYPE_ID_EXECRESULT => write!(f, "{}", self.as_exec_result().unwrap()),
            TYPE_ID_INTEGER    => write!(f, "{}", self.as_integer().unwrap()),
            TYPE_ID_FLOAT      => write!(f, "{}", self.as_float().unwrap()),
            TYPE_ID_RANGE      => write!(f, "{}", self.as_range().unwrap()),

            _ => panic!("Unknown type ID: {}", self.type_id())
        }
    }
}


impl Debug for Value
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
    {
        let error = self.get_error();
        let propagation = self.get_error_propagation();

        write!(f, "(Error: {}, Propagation: {}, value: {})", error, propagation, self)
    }
}
