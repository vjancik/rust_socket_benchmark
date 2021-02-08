// use crate::prelude::*;

#[allow(unused_macros)]
macro_rules! anon {
    ( $( $field_name:ident : $value:expr ),+ ) => {
        {
            // Abusing field_name by using it also as type's name.
            // #[derive(Debug, Clone, Eq, PartialEq, PartialOrd)]
            #[allow(non_camel_case_types)]
            struct Anon<$( $field_name ),*> {
                $(
                    $field_name: $field_name,
                )*
            }
            Anon {
                $(
                    $field_name: $value,
                )*
            }
        }
    }
}

macro_rules! as_variant {
    (struct, { $( $field_name:ident ),+ } , $variant:path, $val:expr ) => { match $val {
        $variant{ $( $field_name ),+ } => anon!{ $( $field_name : $field_name ),+ },
        _ => panic!("Invalid enum variant"),
    }};
    (tuple, 1, $variant:path, $val:expr ) => { match $val {
        $variant(val) => val,
        _ => panic!("Invalid enum variant"),
    }};
    (tuple, 2, $variant:path, $val:expr ) => { match $val {
        $variant(val1, val2) => (val1, val2),
        _ => panic!("Invalid enum variant"),
    }};
}

#[cfg(test)]
mod tests {

    #[derive(Debug)]
    enum TestTuple {
        // OneTuple(usize),
        TwoTuple(usize, usize),
        TestStruct {
            test_field: usize
        }
    }

    #[test]
    fn as_variant_macro() {
        let test_enum = TestTuple::TwoTuple(1,2);
        let test_struct = TestTuple::TestStruct { test_field: 1 };
        println!("{}", as_variant!(tuple, 2, TestTuple::TwoTuple, test_enum).1);
        println!("{}", as_variant!(struct, { test_field }, TestTuple::TestStruct, test_struct).test_field);
    }
}