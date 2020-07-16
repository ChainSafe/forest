use serde::{de,de::Visitor, de::DeserializeOwned,Deserialize,ser, Deserializer, Serialize, Serializer};
use serde_json;
use std::marker::PhantomData;

pub struct JsonType<T> where T : Serialize + DeserializeOwned
{
    val : T
}

impl<T> JsonType<T> where T : Serialize + DeserializeOwned
{
    pub fn new(val : T) -> Self
    {
        JsonType{
            val 
        }
    }
}

struct JsonTypeVisitor<T> where T : Serialize + DeserializeOwned
{
    phantomData : PhantomData<T>
}

impl<T> Serialize for JsonType<T> where T : Serialize + DeserializeOwned {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut json_type_ser = serializer.serialize_struct("JsonType", 1)?;
        let json_val : String = serde_json::to_string(&self.val).unwrap();
        json_type_ser.serialize_field("val",json_val);
        json_type_ser.end()
    }
}

impl<'de,T> Visitor<'de> for JsonTypeVisitor<T> where T : Serialize + DeserializeOwned
{
    type Value = JsonType<T>;
    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("expecrted serialized string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
                    where
                        E: de::Error,
        {
            let val : JsonType<T> = serde_json::from_str(value).unwrap();
            Ok(val)
        }
}
impl<'de,T> Deserialize<'de> for JsonType<T> where T : Serialize + DeserializeOwned  {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
       Deserialize::deserialize(deserializer)
      
    }
}


#[cfg(test)]
mod tests {
    use cid::Cid;
    use super::*;

    #[test]
    fn test_cid_serialize_deserialize() {
        let old = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n";
        let cid = Cid::from_raw_cid(old).unwrap();
        let json_type: JsonType<Cid> = JsonType::new(cid);
        let value = serde_json::to_string(&json_type);
        println!("value {:?}",value)
    }
  
}

