use super::{two_dec, FontInfo, ScaleEntity, TextEntity};
use dxf::entities::{self, AttributeDefinition};
use hex_color::HexColor;
use simple_xml_builder::XMLElement;
use unicode_segmentation::UnicodeSegmentation;
use uuid::Uuid;

/*use parley::{
    Alignment, FontContext, FontWeight, InlineBox, Layout, LayoutContext, PositionedLayoutItem,
    StyleProperty,
};*/

use super::{HAlignment, VAlignment};

// Normaliza cadenas MTEXT (DXF) eliminando códigos de formato y aplicando saltos de línea.
// Maneja casos comunes: \P (newline), \f...\; (fuente), \H...\; (altura),
// \W...\; (ancho), \~ (espacio), \\ (barra invertida literal), \S...\; (apilados -> texto plano).
// IMPORTANTE: Todo lo que está antes del primer ';' se considera código de formato y se elimina.
// El texto real comienza después del primer ';'.
fn normalize_mtext(input: &str) -> String {
    // Primero, encontrar el primer ';' - todo antes de él es código de formato
    let first_semicolon = input.find(';');
    let text_start = if let Some(pos) = first_semicolon {
        pos + 1 // Empezar después del ';'
    } else {
        // Si no hay ';', procesar todo el texto (puede ser texto sin formato)
        0
    };
    
    // Procesar el texto que viene después del primer ';'
    let text_part = &input[text_start..];
    let mut out = String::with_capacity(text_part.len());
    let bytes = text_part.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Elimina llaves que usan muchos CAD para agrupar formato en MTEXT
        if bytes[i] == b'{' || bytes[i] == b'}' {
            i += 1;
            continue;
        }

        if bytes[i] != b'\\' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }

        // Verificar si es una barra invertida escapada
        if i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
            out.push('\\');
            i += 2;
            continue;
        }

        // Verificar si es un código de control especial de un solo carácter
        if i + 1 < bytes.len() {
            match bytes[i + 1] as char {
                'P' => {
                    // Salto de línea
                    out.push('\n');
                    i += 2;
                    continue;
                }
                '~' => {
                    // Espacio no separable
                    out.push(' ');
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }

        // Si el siguiente carácter es una letra, podría ser un código de formato
        if i + 1 < bytes.len() && (bytes[i + 1] as char).is_alphabetic() {
            let start = i;
            i += 2; // Saltar \ y la primera letra
            let mut found_semicolon = false;

            // Consumir letras, números, guiones, guiones bajos, pipes, puntos y espacios hasta encontrar ';'
            // Los puntos son necesarios para números decimales en códigos como \W0.82571;
            while i < bytes.len() {
                let b = bytes[i];
                if b == b';' {
                    // Encontramos el final del código, omitirlo completamente
                    i += 1;
                    found_semicolon = true;
                    break;
                } else if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'|' || b == b' ' || b == b'.' {
                    i += 1;
                } else {
                    // No es parte del código, restaurar todo desde el inicio
                    for j in start..i {
                        out.push(bytes[j] as char);
                    }
                    break;
                }
            }

            // Si encontramos el ';', el código fue eliminado correctamente
            if found_semicolon {
                continue;
            }
        } else {
            // No es un código de formato, mantener la barra invertida
            out.push('\\');
            i += 1;
        }
    }

    out
}

// Información extraída del código de formato MTEXT
#[derive(Debug, Default)]
struct MTextFormatInfo {
    family: Option<String>,
    bold: bool,
    italic: bool,
    point_size: Option<f64>,
    color_index: Option<i32>,
}

// Extrae toda la información de formato desde el primer bloque \f...\; de una cadena MTEXT.
// Ejemplos:
//   {\fGaramond|b0|i1|c0|p18;Sofrel}  -> family="Garamond", italic=true, point_size=18.0
//   {\fSwis721 BlkEx BT|b0|i0|c0|p34;RS485i} -> family="Swis721 BlkEx BT", point_size=34.0
fn extract_mtext_format(input: &str) -> MTextFormatInfo {
    let mut info = MTextFormatInfo::default();
    let bytes = input.as_bytes();
    let mut i = 0usize;
    
    while i + 2 < bytes.len() {
        if bytes[i] == b'\\' && bytes[i + 1] == b'f' {
            // inicio de bloque fuente. Capturamos hasta ';'
            i += 2;
            let start = i;
            while i < bytes.len() && bytes[i] != b';' {
                i += 1;
            }
            let block = &input[start..i];
            // block típico: Family|b0|i0|c0|p34
            let parts: Vec<&str> = block.split('|').collect();
            
            // Primera parte es la familia de fuente
            if let Some(family_part) = parts.first() {
                let family = family_part.trim().trim_matches(['{', '}']);
                if !family.is_empty() {
                    info.family = Some(family.to_string());
                }
            }
            
            // Procesar el resto de las partes
            for part in parts.iter().skip(1) {
                let part = part.trim();
                if part.is_empty() {
                    continue;
                }
                
                // b0 o b1 = bold
                if part.starts_with('b') && part.len() >= 2 {
                    if let Ok(val) = part[1..].parse::<i32>() {
                        info.bold = val != 0;
                    }
                }
                // i0 o i1 = italic
                else if part.starts_with('i') && part.len() >= 2 {
                    if let Ok(val) = part[1..].parse::<i32>() {
                        info.italic = val != 0;
                    }
                }
                // c0, c1, etc. = color index
                else if part.starts_with('c') && part.len() >= 2 {
                    if let Ok(val) = part[1..].parse::<i32>() {
                        info.color_index = Some(val);
                    }
                }
                // p18, p34, etc. = point size
                else if part.starts_with('p') && part.len() >= 2 {
                    if let Ok(val) = part[1..].parse::<f64>() {
                        info.point_size = Some(val);
                    }
                }
            }
            break;
        }
        i += 1;
    }
    
    info
}

#[derive(Debug)]
pub struct DynamicText {
    pub text: String,
    pub info_name: Option<String>,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub rotation: f64,
    pub uuid: Uuid,
    pub h_alignment: HAlignment,
    pub font: FontInfo,
    pub text_from: String,
    pub v_alignment: VAlignment,
    pub frame: bool,
    pub text_width: i32,
    pub keep_visual_rotation: bool,
    pub color: HexColor,
    pub reference_rectangle_width: f64,
    pub original_text_height: f64, // Altura original del texto en unidades DXF
}

impl From<&DynamicText> for XMLElement {
    fn from(txt: &DynamicText) -> Self {
        let mut dtxt_xml = XMLElement::new("dynamic_text");
        // taken from QET_ElementScaler: "ElmtDynText::AsSVGstring"
        //    // Position und Rotationspunkt berechnen:
        //    posx = x + (size/8.0)+4.05 - 0.5;
        //    posy = y + (7.0/5.0*size + 26.0/5.0) - 0.5;
        //    rotx = (-1) * (((size/8.0)+4.05) - 0.5);
        //    roty = (-1) * ((7.0/5.0*size + 26.0/5.0) - 0.5);
        //
        // reversed and slightly modified after looking at the result in element-editor:
        //
        let pt_size: f64 = txt.font.point_size;
        //
        // we need the horizontal alignment and the text-width to move to right x-position:
        // txt.reference_rectangle_width, // should be text-width (Group code 41)
        // txt.attachment_point,  // Group code 71
        //                        // 1 = Top left; 2 = Top center; 3 = Top right
        //                        // 4 = Middle left; 5 = Middle center; 6 = Middle right
        //                        // 7 = Bottom left; 8 = Bottom center; 9 = Bottom right
        //
        //
        // it's just annoying if the value for "reference_rectangle_width" in the dxf is “0.0”...
        //
        // o.k. ... as long as we do not know the real width:
        // "guess" the width by number of characters and font-size:
        //
        let graphene_count = txt.text.graphemes(true).count();
        let txt_width = if txt.reference_rectangle_width > 2.0 {
            txt.reference_rectangle_width
        } else {
            (graphene_count as f64) * pt_size * 0.75
        };

        let x_pos = {
            let x_pos = txt.x + 0.5 - (pt_size / 8.0) - 4.05;
            match txt.h_alignment {
                HAlignment::Left => x_pos,
                HAlignment::Center => x_pos - txt_width / 2.0,
                HAlignment::Right => x_pos - txt_width,
            }
        };
        let y_pos = txt.y + 0.5 - (7.0 / 5.0 * pt_size + 26.0 / 5.0) + pt_size;

        dtxt_xml.add_attribute("x", two_dec(x_pos));
        dtxt_xml.add_attribute("y", two_dec(y_pos));
        dtxt_xml.add_attribute("z", two_dec(txt.z));
        dtxt_xml.add_attribute("rotation", two_dec(txt.rotation));
        dtxt_xml.add_attribute("uuid", format!("{{{}}}", txt.uuid));
        dtxt_xml.add_attribute("font", &txt.font);
        dtxt_xml.add_attribute("Halignment", &txt.h_alignment);
        dtxt_xml.add_attribute("Valignment", &txt.v_alignment);
        dtxt_xml.add_attribute("text_from", &txt.text_from);
        dtxt_xml.add_attribute("frame", txt.frame);
        dtxt_xml.add_attribute("text_width", txt.text_width);
        dtxt_xml.add_attribute("color", txt.color.display_rgb());

        //If I ever add support for other text_from types, element and composite text
        //I'll need to add more smarts here, as there may be some other children components
        //for now since it only supports user_text I'm just statically adding the single child
        //component needed
        //match txt.text_from
        let mut text_xml = XMLElement::new("text");
        text_xml.add_text(&txt.text);
        dtxt_xml.add_child(text_xml);

        if let Some(i_name) = &txt.info_name {
            dtxt_xml.add_attribute("info_name", i_name);
        }

        if txt.keep_visual_rotation {
            dtxt_xml.add_attribute("keep_visual_rotation", txt.keep_visual_rotation);
        }

        dtxt_xml
    }
}

impl ScaleEntity for DynamicText {
    fn scale(&mut self, fact_x: f64, fact_y: f64) {
        self.x *= fact_x;
        self.y *= fact_y;
        // Escalar point_size igual que las coordenadas para mantener la misma relación
        self.font.point_size *= fact_x;
    }

    fn left_bound(&self) -> f64 {
        self.x
    }

    fn right_bound(&self) -> f64 {
        //todo!()
        1.0
    }

    fn top_bound(&self) -> f64 {
        self.y
    }

    fn bot_bound(&self) -> f64 {
        //todo!()
        1.0
    }
}

pub struct DTextBuilder<'a> {
    text: TextEntity<'a>,
    color: Option<HexColor>,
}

impl<'a> DTextBuilder<'a> {
    pub fn from_text(text: &'a entities::Text) -> Self {
        Self {
            text: TextEntity::Text(text),
            color: None,
        }
    }

    pub fn from_mtext(text: &'a entities::MText) -> Self {
        Self {
            text: TextEntity::MText(text),
            color: None,
        }
    }

    pub fn from_attrib(attrib: &'a AttributeDefinition) -> Self {
        Self {
            text: TextEntity::Attrib(attrib),
            color: None,
        }
    }

    pub fn color(self, color: HexColor) -> Self {
        Self {
            color: Some(color),
            ..self
        }
    }




    pub fn build(self) -> DynamicText {
        let (
            x,
            y,
            z,
            rotation,
            style_name,
            text_height,
            value,
            h_alignment,
            v_alignment,
            reference_rectangle_width,
        ) = match self.text {
            TextEntity::Text(txt) => (
                txt.location.x,
                -txt.location.y,
                txt.location.z,
                txt.rotation,
                &txt.text_style_name,
                txt.text_height,
                normalize_mtext(&txt.value), // Normalizar también textos simples
                HAlignment::from(txt.horizontal_text_justification),
                VAlignment::from(txt.vertical_text_justification),
                0.0, // as Placeholder: no "reference_rectangle_width" with Text!!!
            ),
            TextEntity::MText(mtxt) => (
                mtxt.insertion_point.x,
                -mtxt.insertion_point.y,
                mtxt.insertion_point.z,
                mtxt.rotation_angle,
                &mtxt.text_style_name,
                //I'm not sure what the proper value is here for Mtext
                //becuase I haven't actually finished supporting it.
                //I'll put initial text height for now. But i'm not certain
                //exactly what this correlates to. There is also vertical_height,
                //which I would guess is the total vertical height for all the lines
                //it's possible I would need to take the vertical height and divide
                //by the number of lines to get the value I need....I'm not sure yet
                mtxt.initial_text_height,
                //There are 2 text fields on MTEXT, .text a String and .extended_text a Vec<String>
                //Most of the example files I have at the moment are single line MTEXT.
                //I edited one of them in QCad, and added a few lines. The value came through in the text field
                //with extended_text being empty, and the newlines were deliniated by '\\P'...I might need to look
                //the spec a bit to determine what it says for MTEXT, but for now, I'll just assume this is correct
                //So looking at the spec, yes '\P' is the MTEXT newline essentially. There is a bunch of MTEXT
                //inline codes that can be found at https://ezdxf.readthedocs.io/en/stable/dxfentities/mtext.html
                //The extended text is code point 3 in the dxf spec which just says: "Additional text (always in 250-character chunks) (optional)"
                //and Code point 1 the normal text value says: "Text string. If the text string is less than 250 characters, all characters appear
                //in group 1. If the text string is greater than 250 characters, the string is divided into 250-character chunks, which appear in
                //one or more group 3 codes. If group 3 codes are used, the last group is a group 1 and has fewer than 250 characters"
                {
                    let mut raw = mtxt.extended_text.join("");
                    raw.push_str(&mtxt.text);
                    normalize_mtext(&raw)
                },
                HAlignment::from(mtxt.attachment_point),
                VAlignment::from(mtxt.attachment_point),
                mtxt.reference_rectangle_width,
            ),
            TextEntity::Attrib(attrib) => (
                attrib.location.x,
                -attrib.location.y,
                attrib.location.z,
                attrib.rotation,
                &attrib.text_style_name,
                attrib.text_height,
                attrib.value.clone(),
                HAlignment::from(attrib.horizontal_text_justification),
                VAlignment::from(attrib.vertical_text_justification),
                0.0, // as Placeholder: not need to check if Attrib has something similar
            ),
        };

        // Create a FontContext (font database) and LayoutContext (scratch space).
        // These are both intended to be constructed rarely (perhaps even once per app):
        /*let mut font_cx = FontContext::new();
        let mut layout_cx = LayoutContext::new();

        // Create a `RangedBuilder` or a `TreeBuilder`, which are used to construct a `Layout`.
        const DISPLAY_SCALE : f32 = 1.0;
        let mut builder = layout_cx.ranged_builder(&mut font_cx, &value, DISPLAY_SCALE);

        // Set default styles that apply to the entire layout
        builder.push_default(StyleProperty::LineHeight(1.3));
        builder.push_default(StyleProperty::FontSize((text_height * self.txt_sc_factor.unwrap()).round() as f32));

        // Build the builder into a Layout
        let mut layout: Layout<()> = builder.build(&value);

        // Run line-breaking and alignment on the Layout
        const MAX_WIDTH : Option<f32> = Some(1000.0);
        layout.break_all_lines(MAX_WIDTH);
        layout.align(MAX_WIDTH, Alignment::Start);

        let calc_width = layout.width();
        let calc_height = layout.height();
        dbg!(&value);
        dbg!(calc_width);
        dbg!(calc_height);*/

        /*dbg!(&value);
        dbg!(&y);
        dbg!(&self.text);*/
        // Extraer información de formato desde el bloque \f...\; del MTEXT/TEXT
        let format_info = match self.text {
            TextEntity::MText(mtxt) => {
                let mut raw = mtxt.extended_text.join("");
                raw.push_str(&mtxt.text);
                extract_mtext_format(&raw)
            }
            TextEntity::Text(txt) => {
                extract_mtext_format(&txt.value)
            }
            _ => MTextFormatInfo::default(),
        };

        // Determinar el estilo de fuente basado en bold e italic
        use super::FontStyle;
        let font_style = if format_info.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        };

        // Determinar el peso de la fuente (weight) basado en bold
        // weight típicamente: 50 = normal, 75 = bold
        let font_weight = if format_info.bold { 75 } else { 50 };

        DynamicText {
            //x: x - (calc_width as f64/2.0),
            x,
            y,
            z,
            rotation: if rotation.abs().round() as i64 % 360 != 0 {
                rotation - 180.0
            } else {
                0.0
            },
            uuid: Uuid::new_v4(),
            font: {
                // El text_height del DXF viene en unidades del DXF
                // No lo escalamos aquí porque se escalará en dtext.scale() junto con las coordenadas
                // Esto asegura que el texto se escale con la misma relación que el resto del dibujo
                let text_height_pt = text_height;
                
                let mut f = if style_name == "STANDARD" {
                    FontInfo {
                        point_size: text_height_pt,
                        ..Default::default()
                    }
                } else {
                    // mismo comportamiento que STANDARD, pero permitimos sobrescribir la familia
                    FontInfo {
                        point_size: text_height_pt,
                        ..Default::default()
                    }
                };
                // Aplicar información extraída del formato
                if let Some(fam) = format_info.family {
                    f.family = fam;
                }
                f.style = font_style;
                f.weight = font_weight;
                f
            },
            reference_rectangle_width, //liest aus der dxf-Datei!!!
            h_alignment,
            v_alignment,
            text_from: "UserText".into(),
            frame: false,
            text_width: -1,
            color: {
                // Si hay un color_index en el formato, usarlo; si no, usar el color del builder
                if let Some(color_idx) = format_info.color_index {
                    // Los índices de color DXF van de 0-255, donde 0 es "ByBlock", 256 es "ByLayer"
                    // Para simplificar, usamos el color del builder si color_index es 0 o inválido
                    if color_idx > 0 && color_idx < 256 {
                        // Convertir índice DXF a color (simplificado)
                        HexColor::from_u32(color_idx as u32)
                    } else {
                        self.color.unwrap_or(HexColor::BLACK)
                    }
                } else {
                    self.color.unwrap_or(HexColor::BLACK)
                }
            },
            original_text_height: text_height, // Guardar el text_height original del DXF
            text: value,
            keep_visual_rotation: false,
            info_name: None,
        }
    }
}
