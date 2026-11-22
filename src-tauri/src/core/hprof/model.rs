use std::collections::{BTreeMap, HashMap};

pub const MAX_DOMINATOR_OBJECTS: usize = 1_500_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    Object,
    Boolean,
    Char,
    Float,
    Double,
    Byte,
    Short,
    Int,
    Long,
}

impl FieldType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            2 => Some(Self::Object),
            4 => Some(Self::Boolean),
            5 => Some(Self::Char),
            6 => Some(Self::Float),
            7 => Some(Self::Double),
            8 => Some(Self::Byte),
            9 => Some(Self::Short),
            10 => Some(Self::Int),
            11 => Some(Self::Long),
            _ => None,
        }
    }

    pub fn size(self, id_size: usize) -> usize {
        match self {
            Self::Object => id_size,
            Self::Boolean | Self::Byte => 1,
            Self::Char | Self::Short => 2,
            Self::Float | Self::Int => 4,
            Self::Double | Self::Long => 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
    pub name: String,
    pub name_id: u64,
    pub ty: FieldType,
    pub weak: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ClassMeta {
    pub class_id: u64,
    pub name_id: u64,
    pub name: String,
    pub super_id: u64,
    pub loader_id: u64,
    pub instance_size: u64,
    pub instance_fields: Vec<FieldDef>,
    pub static_refs: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjKind {
    Instance,
    ObjectArray(u32),
    PrimitiveArray(u8, u32),
    ClassObject,
}

#[derive(Debug, Clone)]
pub struct ObjectRecord {
    pub id: u64,
    pub class_id: u64,
    pub class_idx: u32,
    pub class_object: bool,
    pub shallow: u64,
    pub kind: ObjKind,
    pub refs_start: u32,
    pub refs_len: u32,
}

#[derive(Debug, Default)]
pub struct Pass1 {
    pub version: String,
    pub strings: HashMap<u64, String>,
    pub classes: Vec<ClassMeta>,
    pub class_index: HashMap<u64, usize>,
    pub class_names: HashMap<u64, u64>,
    pub objects: Vec<ObjectRecord>,
    pub object_index: HashMap<u64, u32>,
    pub roots: Vec<(u64, &'static str)>,
    pub root_counts: BTreeMap<String, u64>,
    pub edges: Vec<u32>,
    pub weak_targets: Vec<u32>,
    pub layouts: Vec<Vec<FieldDef>>,
}

impl Pass1 {
    pub fn ensure_class(&mut self, class_id: u64, name_id: u64) -> usize {
        if let Some(&index) = self.class_index.get(&class_id) {
            if self.classes[index].name_id == 0 {
                self.classes[index].name_id = name_id;
            }
            return index;
        }
        let index = self.classes.len();
        self.classes.push(ClassMeta {
            class_id,
            name_id,
            ..Default::default()
        });
        self.class_index.insert(class_id, index);
        index
    }

    pub fn add_object(
        &mut self,
        id: u64,
        class_id: u64,
        class_object: bool,
        shallow: u64,
        kind: ObjKind,
    ) {
        if self.object_index.contains_key(&id) {
            return;
        }
        let index = self.objects.len() as u32;
        self.objects.push(ObjectRecord {
            id,
            class_id,
            class_idx: u32::MAX,
            class_object,
            shallow,
            kind,
            refs_start: 0,
            refs_len: 0,
        });
        self.object_index.insert(id, index);
    }

    pub fn object_class_name(&self, object: &ObjectRecord) -> String {
        if object.class_object {
            return "java.lang.Class".to_string();
        }
        if let Some(class) = self.classes.get(object.class_idx as usize) {
            return class.name.clone();
        }
        match object.kind {
            ObjKind::PrimitiveArray(ty, _) => primitive_array_name(ty).to_string(),
            ObjKind::ObjectArray(_) => "[Ljava.lang.Object;".to_string(),
            _ => format!("unknown 0x{:x}", object.class_id),
        }
    }

    pub fn loader_id_for(&self, object: &ObjectRecord) -> Option<u64> {
        if object.class_object {
            return Some(0);
        }
        self.classes
            .get(object.class_idx as usize)
            .map(|class| class.loader_id)
    }

    pub fn loader_label(&self, loader_id: Option<u64>) -> String {
        match loader_id {
            None => "Unknown".to_string(),
            Some(0) => "Bootstrap".to_string(),
            Some(id) => {
                if let Some(&index) = self.object_index.get(&id) {
                    let object = &self.objects[index as usize];
                    format!("{} [{}]", self.object_class_name(object), short_id(id))
                } else {
                    format!("loader [{}]", short_id(id))
                }
            }
        }
    }
}

pub fn header_size(id_size: usize) -> u64 {
    if id_size == 8 {
        16
    } else {
        12
    }
}

pub fn short_id(id: u64) -> String {
    format!("{id:x}")
}

pub fn primitive_array_name(ty: u8) -> &'static str {
    match ty {
        4 => "[Z",
        5 => "[C",
        6 => "[F",
        7 => "[D",
        8 => "[B",
        9 => "[S",
        10 => "[I",
        11 => "[J",
        _ => "[?",
    }
}
