mod constants;
mod serialize;
mod tokenizer;
mod value_tree_builder;

pub use serialize::Value;
pub use serialize::DateTime;
pub use serialize::Serializer;

pub use tokenizer::Tokenizer;
pub use value_tree_builder::ValueTreeBuilder;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize () {
        // serialized frps data
        let data = vec![0xca, 0x11, 0x03 ,0x00];

        // result Value tree
        let mut tree = value_tree_builder::ValueTreeBuilder::new();

        // Tokenizer
        let mut tokenizer = tokenizer::Tokenizer::new();
        let res = tokenizer.parse(&data, &mut tree);
    
        assert_eq!(res.is_ok(), true);
        assert_eq!(tree.major_version, 3);
        assert_eq!(tree.minor_version, 0);
    }
}