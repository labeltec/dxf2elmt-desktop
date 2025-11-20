use dxf::entities::{AttributeDefinition, Entity, EntityType};
use dxf::entities::{LwPolyline, Polyline};
use dxf::enums::{AttachmentPoint, HorizontalTextJustification, Units, VerticalTextJustification};
use dxf::{Block, Drawing};
use dynamictext::DTextBuilder;
use hex_color::HexColor;
use itertools::Itertools;
use simple_xml_builder::XMLElement;
use std::convert::TryFrom;
use std::f64::consts::PI;
use std::fmt::Display;
use uuid::Uuid;

use tracing::{error, info, span, trace, Level};

pub mod arc;
pub use arc::Arc;

pub mod line;
pub use line::{Leader, Line};

pub mod text;
pub use text::Text;

pub mod dynamictext;
pub use dynamictext::DynamicText;

pub mod polygon;
pub use polygon::Polygon;

pub mod ellipse;
pub use ellipse::Ellipse;

fn find_block<'a>(drw: &'a Drawing, name: &str) -> Option<&'a Block> {
    //this is ugly there has to be a cleaner way to filter this....but for my first attempt at pulling the
    //blocks out of the drawing it works.
    //I mean would this ever return more than 1? I would assume block names have to be unique?
    //but maybe not, the blocks have a handle, which is a u64. There is a get by handle function
    //but not a get by name function....maybe the handle is what is unique and there can be duplicate names?
    //a quick glance through the dxf code it looks like the handle might be given to the library user when inserting
    //an entity? So I don't think there is any easy way to get the handle
    drw.blocks().filter(|bl| bl.name == name).take(1).next()
}

#[derive(Debug)]
enum Either<L, R> {
    Left(L),
    Right(R),
}

#[derive(Debug)]
pub struct Definition {
    r#type: ItemType,
    width: i64,
    height: i64,
    hotspot_x: i64,
    hotspot_y: i64,
    version: String,
    link_type: LinkType,
    uuid: ElmtUuid,
    names: Names,
    element_infos: Option<ElemInfos>,
    informations: &'static str,
    pub description: Description,
    //counts
}

//Since the ScaleEntity trait was added to all the objects/elements
//and I need to add the get bounds to all it probably makes sense to have

//them all within the same trait instead of multiple traits, as a collective
//set of functions needed by the objects...but I should probably come up with
//a better trait name then. For now I'll leave it and just get the code working
trait ScaleEntity {
    fn scale(&mut self, fact_x: f64, fact_y: f64);

    fn left_bound(&self) -> f64;
    fn right_bound(&self) -> f64;

    fn top_bound(&self) -> f64;
    fn bot_bound(&self) -> f64;
}

trait Circularity {
    fn is_circular(&self) -> bool;

    fn match_range() -> std::ops::RangeInclusive<f64> {
        //this boundary of 2% has been chosen arbitrarily, I might adjust this later
        //I know in one of my sample files, I'm getting a value of 0.99....
        //since Associated Constants in a trait can't have a default value
        //I'm using this function that defaults to a constant range of 2%
        //Then I could also easily overwrite it if I wanted to change the tolerance
        //for a specific type
        0.98..=1.02
    }
}

impl Circularity for Polyline {
    fn is_circular(&self) -> bool {
        let poly_perim: f64 = {
            let tmp_pts: Vec<dxf::Point> = self.vertices().map(|v| v.clone().location).collect();
            let len = tmp_pts.len();
            tmp_pts
                .into_iter()
                .circular_tuple_windows()
                .map(|(fst, sec)| ((fst.x - sec.x).powf(2.0) + (fst.y - sec.y).powf(2.0)).sqrt())
                .take(len)
                .sum()
        };

        let poly_area = {
            //because instead of being able to access the Vec like in LwPolyline, vertices() returns
            //an iter of dxf Vertex's which don't implement clone so I can't use circular_tuple_windows
            //there is probably a cleaner way of iterating over this, but it's late, I'm getting tired
            //and just want to see if this basic idea will work on my sample file, or see if I'm chasing
            //up the wrong tree.
            let tmp_pts: Vec<dxf::Point> = self.vertices().map(|v| v.clone().location).collect();
            let len = tmp_pts.len();
            let mut poly_area: f64 = tmp_pts
                .into_iter()
                .circular_tuple_windows()
                .map(|(fst, sec)| (fst.x * sec.y) - (fst.y * sec.x))
                .take(len)
                .sum();
            poly_area /= 2.0;
            poly_area.abs()
        };
        let t_ratio = 4.0 * PI * poly_area / poly_perim.powf(2.0);

        Self::match_range().contains(&t_ratio)
    }
}

impl Circularity for LwPolyline {
    fn is_circular(&self) -> bool {
        let poly_perim: f64 = self
            .vertices
            .iter()
            .circular_tuple_windows()
            .map(|(fst, sec)| {
                ((fst.x - sec.x).powf(2.0) + (fst.y - sec.y).powf(2.0))
                    .abs()
                    .sqrt()
            })
            .take(self.vertices.len())
            .sum();

        let poly_area = {
            let mut poly_area: f64 = self
                .vertices
                .iter()
                .circular_tuple_windows()
                .map(|(fst, sec)| (fst.x * sec.y) - (fst.y * sec.x))
                .take(self.vertices.len())
                .sum();
            poly_area /= 2.0;
            poly_area.abs()
        };
        let t_ratio = 4.0 * PI * poly_area / poly_perim.powf(2.0);

        Self::match_range().contains(&t_ratio)
    }
}

impl Definition {
    pub fn new(name: impl Into<String>, spline_step: u32, px_per_mm: f64, drw: &Drawing) -> Self {
        /*for st in drw.styles() {
            dbg!(st);
        }*/
        let final_scale_factor = Self::apply_unit_conversion(drw.header.default_drawing_units, px_per_mm);
        let description = {
            let mut description: Description = (drw, spline_step, px_per_mm).into();
            description.scale(final_scale_factor, final_scale_factor);
            description
        };

        //The below calculation for width and hotspot_x are taken from the qet source code
        let (width, hotspot_x) = {
            let tmp_width = description.right_bound() - description.left_bound();
            let int_width = tmp_width.round() as i64;
            let upwidth = ((int_width / 10) * 10) + 10;
            let xmargin = (upwidth as f64 - tmp_width).round();

            let width = if int_width % 10 > 6 {
                upwidth + 10
            } else {
                upwidth
            };

            (
                width,
                -((description.left_bound() - (xmargin / 2.0)).round() as i64),
            )
        };

        //The below calculation for height and hotspot_y are taken from the qet source code
        let (height, hotspot_y) = {
            let tmp_height = description.bot_bound() - description.top_bound();
            let int_height = tmp_height.round() as i64;
            let upheight = ((int_height / 10) * 10) + 10;
            let ymargin = (upheight as f64 - tmp_height).round();

            let height = if int_height % 10 > 6 {
                upheight + 10
            } else {
                upheight
            };

            (
                height,
                -((description.top_bound() - (ymargin / 2.0)).round() as i64),
            )
        };

        Definition {
            r#type: ItemType::Element,
            width,
            height,
            hotspot_x,
            hotspot_y,
            version: "0.8.0".into(),
            link_type: LinkType::Simple,
            uuid: Uuid::new_v4().into(),
            names: Names {
                names: vec![Name {
                    lang: "en".into(),
                    value: name.into(), //need to truncate the extension
                }],
            },
            element_infos: None,
            informations: "Created using dxf2elmt!",
            description,
        }
    }

    // Apply unit conversion to the scale factor
    // Convert from DXF unit to millimeters, then apply px_per_mm to get pixels
    // For example: if DXF is in meters and px_per_mm=2.0, we convert 1 meter = 1000 mm, then 1000 mm * 2 px/mm = 2000 px
    fn apply_unit_conversion(unit: Units, px_per_mm: f64) -> f64 {
        // Unit conversions taken from: https://www.unitconverters.net/length-converter.html
        // Convert DXF units to millimeters, then multiply by px_per_mm to get pixels
        let unit_to_mm_factor = match unit {
            Units::Unitless => 1.0, // No conversion for unitless
            Units::Inches => 25.4, // 1 inch = 25.4 mm
            Units::Feet => 304.8, // 1 foot = 304.8 mm
            Units::Miles | Units::USSurveyMile => 1_609_344.0, // 1 mile = 1,609,344 mm
            Units::Millimeters => 1.0,
            Units::Centimeters => 10.0, // 1 cm = 10 mm
            Units::Meters => 1_000.0, // 1 m = 1000 mm
            Units::Kilometers => 1_000_000.0, // 1 km = 1,000,000 mm
            Units::Microinches => 0.000_025_4, // 1 microinch = 0.0000254 mm
            Units::Mils => 0.025_4, // 1 mil = 0.0254 mm
            Units::Yards => 914.4, // 1 yard = 914.4 mm
            Units::Angstroms => 1.0e-7, // 1 angstrom = 1e-7 mm
            Units::Nanometers => 1.0e-6, // 1 nm = 1e-6 mm
            Units::Microns => 0.001, // 1 micron = 0.001 mm
            Units::Decimeters => 100.0, // 1 dm = 100 mm
            Units::Decameters => 10_000.0, // 1 dam = 10,000 mm
            Units::Hectometers => 100_000.0, // 1 hm = 100,000 mm
            Units::Gigameters => 1.0e12, // 1 Gm = 1e12 mm
            Units::AstronomicalUnits => 149_597_870_691_000.0, // 1 AU = 149,597,870,691,000 mm
            Units::LightYears => 9_460_730_472_580_800_000.0, // 1 ly = 9,460,730,472,580,800,000 mm
            Units::Parsecs => 30_856_775_814_671_900_000.0, // 1 pc = 30,856,775,814,671,900,000 mm
            Units::USSurveyFeet => 304.800_609_6, // 1 US survey foot = 304.8006096 mm
            Units::USSurveyInch => 25.400_050_8, // 1 US survey inch = 25.4000508 mm
            Units::USSurveyYard => 914.401_828_8, // 1 US survey yard = 3 * 304.8006096 mm
        };
        // Convert to mm, then to pixels: unit_value * unit_to_mm_factor * px_per_mm
        unit_to_mm_factor * px_per_mm
    }
}

impl From<&Definition> for XMLElement {
    fn from(def: &Definition) -> Self {
        let mut def_xml = XMLElement::new("definition");
        def_xml.add_attribute("height", def.height);
        def_xml.add_attribute("width", def.width);
        def_xml.add_attribute("hotspot_x", def.hotspot_x);
        def_xml.add_attribute("hotspot_y", def.hotspot_y);
        def_xml.add_attribute("version", &def.version);
        def_xml.add_attribute("link_type", &def.link_type);
        def_xml.add_attribute("type", &def.r#type);

        def_xml.add_child((&def.uuid).into());
        def_xml.add_child((&def.names).into());
        if let Some(einfos) = &def.element_infos {
            def_xml.add_child(einfos.into());
        }

        let mut info_elmt = XMLElement::new("informations");
        info_elmt.add_text(def.informations);
        def_xml.add_child(info_elmt);

        def_xml.add_child((&def.description).into());

        def_xml
    }
}

#[derive(Debug)]
pub enum Objects {
    Arc(Arc),
    Ellipse(Ellipse),
    Polygon(Polygon),
    DynamicText(DynamicText),
    Text(Text),
    Line(Line),
    //Terminal(Terminal),
    Group(Vec<Objects>),
}

impl Objects {
    pub fn descendants(&self) -> Descendants<'_> {
        Descendants {
            stack: vec![self.children()],
        }
    }

    pub fn children(&self) -> Children<'_> {
        match self {
            Objects::Group(l) => Children { slice: l.iter() },
            _ => Children { slice: [].iter() },
        }
    }
}

pub(crate) struct Descendants<'a> {
    stack: Vec<Children<'a>>,
}

impl<'a> Iterator for Descendants<'a> {
    type Item = &'a Objects;
    fn next(&mut self) -> Option<Self::Item> {
        //let iter_span = span!(Level::TRACE, "Iterating Object Descendants");
        //let _span_guard = iter_span.enter();
        while let Some(last) = self.stack.last_mut() {
            if let Some(obj) = last.next() {
                //trace!("Found more children");
                self.stack.push(obj.children());
                return Some(obj);
            }

            self.stack.pop();
        }
        None
    }
}

pub(crate) struct Children<'a> {
    slice: std::slice::Iter<'a, Objects>,
}

impl<'a> Iterator for Children<'a> {
    type Item = &'a Objects;
    fn next(&mut self) -> Option<Self::Item> {
        self.slice.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.slice.len(), None)
    }
}

impl ScaleEntity for Objects {
    fn scale(&mut self, fact_x: f64, fact_y: f64) {
        match self {
            Objects::Arc(arc) => arc.scale(fact_x, fact_y),
            Objects::Ellipse(ellipse) => ellipse.scale(fact_x, fact_y),
            Objects::Polygon(polygon) => polygon.scale(fact_x, fact_y),
            Objects::DynamicText(dynamic_text) => dynamic_text.scale(fact_x, fact_y),
            Objects::Text(text) => text.scale(fact_x, fact_y),
            Objects::Line(line) => line.scale(fact_x, fact_y),
            Objects::Group(vec) => vec.iter_mut().for_each(|ob| ob.scale(fact_x, fact_y)),
        }
    }

    fn left_bound(&self) -> f64 {
        match self {
            Objects::Arc(arc) => arc.left_bound(),
            Objects::Ellipse(ellipse) => ellipse.left_bound(),
            Objects::Polygon(polygon) => polygon.left_bound(),
            Objects::DynamicText(dynamic_text) => dynamic_text.left_bound(),
            Objects::Text(text) => text.left_bound(),
            Objects::Line(line) => line.left_bound(),
            Objects::Group(vec) => {
                let lb = vec.iter().min_by(|ob1, ob2| {
                    ob1.left_bound()
                        .partial_cmp(&ob2.left_bound())
                        .unwrap_or(std::cmp::Ordering::Greater)
                });

                if let Some(lb) = lb {
                    lb.left_bound()
                } else {
                    0.0
                }
            }
        }
    }

    fn right_bound(&self) -> f64 {
        match self {
            Objects::Arc(arc) => arc.right_bound(),
            Objects::Ellipse(ellipse) => ellipse.right_bound(),
            Objects::Polygon(polygon) => polygon.right_bound(),
            Objects::DynamicText(dynamic_text) => dynamic_text.right_bound(),
            Objects::Text(text) => text.right_bound(),
            Objects::Line(line) => line.right_bound(),
            Objects::Group(vec) => {
                let rb = vec.iter().max_by(|ob1, ob2| {
                    ob1.right_bound()
                        .partial_cmp(&ob2.right_bound())
                        .unwrap_or(std::cmp::Ordering::Less)
                });

                if let Some(rb) = rb {
                    rb.right_bound()
                } else {
                    0.0
                }
            }
        }
    }

    fn top_bound(&self) -> f64 {
        match self {
            Objects::Arc(arc) => arc.top_bound(),
            Objects::Ellipse(ellipse) => ellipse.top_bound(),
            Objects::Polygon(polygon) => polygon.top_bound(),
            Objects::DynamicText(dynamic_text) => dynamic_text.top_bound(),
            Objects::Text(text) => text.top_bound(),
            Objects::Line(line) => line.top_bound(),
            Objects::Group(vec) => {
                let tb = vec.iter().min_by(|ob1, ob2| {
                    ob1.top_bound()
                        .partial_cmp(&ob2.top_bound())
                        .unwrap_or(std::cmp::Ordering::Greater)
                });

                if let Some(tb) = tb {
                    tb.top_bound()
                } else {
                    0.0
                }
            }
        }
    }

    fn bot_bound(&self) -> f64 {
        match self {
            Objects::Arc(arc) => arc.bot_bound(),
            Objects::Ellipse(ellipse) => ellipse.bot_bound(),
            Objects::Polygon(polygon) => polygon.bot_bound(),
            Objects::DynamicText(dynamic_text) => dynamic_text.bot_bound(),
            Objects::Text(text) => text.bot_bound(),
            Objects::Line(line) => line.bot_bound(),
            Objects::Group(vec) => {
                let bb = vec.iter().max_by(|ob1, ob2| {
                    ob1.bot_bound()
                        .partial_cmp(&ob2.bot_bound())
                        .unwrap_or(std::cmp::Ordering::Less)
                });

                if let Some(bb) = bb {
                    bb.bot_bound()
                } else {
                    0.0
                }
            }
        }
    }
}

#[derive(Debug)]
struct ScaleFactor {
    x: f64,
    y: f64,
}

impl Default for ScaleFactor {
    fn default() -> Self {
        ScaleFactor { x: 1.0, y: 1.0 }
    }
}

#[derive(Default, Debug)]
pub(crate) struct Offset {
    x: f64,
    y: f64,
}

#[derive(Debug)]
pub struct ObjectsBuilder<'a> {
    ent: &'a Entity,
    spline_step: u32,
    px_per_mm: f64,
    blocks: &'a [&'a Block],
    offset: Offset,
    scale_fact: ScaleFactor,
}

impl<'a> ObjectsBuilder<'a> {
    pub fn new(ent: &'a Entity, spline_step: u32, px_per_mm: f64) -> Self {
        Self {
            ent,
            spline_step,
            px_per_mm,
            blocks: &[],
            offset: Offset::default(),
            scale_fact: ScaleFactor::default(),
        }
    }

    pub fn blocks(self, blocks: &'a [&'a Block]) -> Self {
        Self { blocks, ..self }
    }

    pub fn offsets(self, x: f64, y: f64) -> Self {
        Self {
            offset: Offset { x, y },
            ..self
        }
    }

    pub fn scaling(self, fact_x: f64, fact_y: f64) -> Self {
        Self {
            scale_fact: ScaleFactor {
                x: fact_x,
                y: fact_y,
            },
            ..self
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn build(self) -> Result<Objects, &'static str /*add better error later*/> {
        match &self.ent.specific {
            EntityType::Circle(circle) => {
                let mut ellipse: Ellipse = circle.into();

                ellipse.scale(self.scale_fact.x, self.scale_fact.y);
                ellipse.x += self.offset.x;
                ellipse.y -= self.offset.y;
                Ok(Objects::Ellipse(ellipse))
            }
            EntityType::Line(line) => {
                let mut line: Line = line.into();

                line.scale(self.scale_fact.x, self.scale_fact.y);

                line.x1 += self.offset.x;
                line.y1 -= self.offset.y;

                line.x2 += self.offset.x;
                line.y2 -= self.offset.y;

                Ok(Objects::Line(line))
            }
            EntityType::Arc(arc) => {
                let mut arc: Arc = arc.into();

                arc.scale(self.scale_fact.x, self.scale_fact.y);

                arc.x += self.offset.x;
                arc.y -= self.offset.y;

                Ok(Objects::Arc(arc))
            }
            EntityType::Spline(spline) => {
                let mut poly: Polygon = (spline, self.spline_step).into();

                match poly.coordinates.len() {
                    0 | 1 => Err("Error removing empty Spline"),
                    //I'll need to improve my understanding of splines and the math here
                    //to make sure I do this correctly.
                    //2 => //convert to line
                    _ => {
                        poly.scale(self.scale_fact.x, self.scale_fact.y);
                        for cord in &mut poly.coordinates {
                            cord.x += self.offset.x;
                            cord.y -= self.offset.y;
                        }

                        Ok(Objects::Polygon(poly))
                    }
                }
            }
            EntityType::Text(text) => {
                Ok(
                    //right now the dxf2elmt defaults to making all text Static Text...
                    //it was requested by the QET devs to add in support for Dynamic text
                    //which was added, but it defaults to OFF, and QET doesn't pass the parameter
                    //to enable it...I'm wondering if it makes more sense to default to use dynamic text
                    //for now I'll set it to use dynamic text, and once I get the CLI flag passing through
                    //I might change the default parameter to use Dynamic Text
                    if false {
                        //how best to pass in the flag for dynamic text or not....should the flag also default to true?
                        let mut text: Text = (
                            text,
                            HexColor::from_u32(self.ent.common.color_24_bit as u32),
                        )
                            .into();

                        // Aplicar px_per_mm
                        // El text_height viene en unidades DXF, y se escala igual que las coordenadas
                        // scale_fact ya incluye la conversión de unidades DXF a píxeles (via apply_unit_conversion)
                        // Calculamos el point_size inicial sin escalar (se escalará en text.scale())
                        text.font.point_size = text.original_text_height;

                        // Escalar coordenadas y point_size con el mismo factor
                        text.scale(self.scale_fact.x, self.scale_fact.y);

                        text.x += self.offset.x;
                        text.y -= self.offset.y;

                        Objects::Text(text)
                    } else {
                        let mut dtext = DTextBuilder::from_text(text)
                            .color(HexColor::from_u32(self.ent.common.color_24_bit as u32))
                            .build();

                        dtext.scale(self.scale_fact.x, self.scale_fact.y);

                        dtext.x += self.offset.x;
                        dtext.y -= self.offset.y;

                        Objects::DynamicText(dtext)
                    },
                )
            }
            EntityType::Ellipse(ellipse) => {
                let mut ellipse: Ellipse = ellipse.into();

                ellipse.scale(self.scale_fact.x, self.scale_fact.y);
                ellipse.x += self.offset.x;
                ellipse.y -= self.offset.y;

                Ok(Objects::Ellipse(ellipse))
            }
            EntityType::MText(mtext) => {
                Ok(
                    //right now the dxf2elmt defaults to making all text Static Text...
                    //it was requested by the QET devs to add in support for Dynamic text
                    //which was added, but it defaults to OFF, and QET doesn't pass the parameter
                    //to enable it...I'm wondering if it makes more sense to default to use dynamic text
                    //for now I'll set it to use dynamic text, and once I get the CLI flag passing through
                    //I might change the default parameter to use Dynamic Text
                    if false {
                        //how best to pass in the flag for dynamic text or not....should the flag also default to true?
                        /*let mut text: Text =
                            (mtext, HexColor::from_u32(ent.common.color_24_bit as u32)).into();
                            text.x += offset_x;
                            text.y -= offset_y;
                        Objects::Text(text)*/
                        todo!();
                    } else {
                        let mut dtext = DTextBuilder::from_mtext(mtext)
                            .color(HexColor::from_u32(self.ent.common.color_24_bit as u32))
                            .build();

                        dtext.scale(self.scale_fact.x, self.scale_fact.y);

                        dtext.x += self.offset.x;
                        dtext.y -= self.offset.y;

                        Objects::DynamicText(dtext)
                    },
                )
            }
            EntityType::Polyline(polyline) => match polyline.__vertices_and_handles.len() {
                0 | 1 => Err("Error empty Polyline"),
                2 => {
                    let mut line = Line::try_from(polyline)?;

                    line.scale(self.scale_fact.x, self.scale_fact.x);

                    line.x1 += self.offset.x;
                    line.y1 -= self.offset.y;

                    line.x2 += self.offset.x;
                    line.y2 -= self.offset.y;

                    Ok(Objects::Line(line))
                }
                _ => {
                    if let Ok(mut ellipse) = Ellipse::try_from(polyline) {
                        ellipse.scale(self.scale_fact.x, self.scale_fact.y);

                        ellipse.x += self.offset.x;
                        ellipse.y -= self.offset.y;

                        Ok(Objects::Ellipse(ellipse))
                    } else {
                        let mut poly: Polygon = polyline.into();

                        poly.scale(self.scale_fact.x, self.scale_fact.y);

                        for cord in &mut poly.coordinates {
                            cord.x += self.offset.x;
                            cord.y -= self.offset.y;
                        }

                        Ok(Objects::Polygon(poly))
                    }
                }
            },
            EntityType::LwPolyline(lwpolyline) => match lwpolyline.vertices.len() {
                0 | 1 => Err("Error empty LwPolyline"),
                2 => {
                    let mut line = Line::try_from(lwpolyline)?;

                    line.scale(self.scale_fact.x, self.scale_fact.y);

                    line.x1 += self.offset.x;
                    line.y1 -= self.offset.y;

                    line.x2 += self.offset.x;
                    line.y2 -= self.offset.y;

                    Ok(Objects::Line(line))
                }
                _ => {
                    if let Ok(mut ellipse) = Ellipse::try_from(lwpolyline) {
                        ellipse.scale(self.scale_fact.x, self.scale_fact.y);

                        ellipse.x += self.offset.x;
                        ellipse.y -= self.offset.y;

                        Ok(Objects::Ellipse(ellipse))
                    } else {
                        let mut poly: Polygon = lwpolyline.into();

                        poly.scale(self.scale_fact.x, self.scale_fact.y);

                        for cord in &mut poly.coordinates {
                            cord.x += self.offset.x;
                            cord.y -= self.offset.y;
                        }

                        Ok(Objects::Polygon(poly))
                    }
                }
            },
            EntityType::Solid(solid) => {
                let mut poly: Polygon = solid.into();

                poly.scale(self.scale_fact.x, self.scale_fact.y);

                for cord in &mut poly.coordinates {
                    cord.x += self.offset.x;
                    cord.y -= self.offset.y;
                }

                Ok(Objects::Polygon(poly))
            }
            EntityType::Insert(ins) => {
                //info!("Found an Insert Block: {ins:?}");
                info!("Found an Insert Block: {}", &ins.name);
                let Some(block) = self.blocks.iter().find(|bl| bl.name == ins.name) else {
                    error!("Block {} not found", ins.name);
                    return Err("Block Not Found");
                };
                trace!(
                    "Base Point: x: {} / y: {}",
                    block.base_point.x,
                    block.base_point.y
                );

                trace!("Creating Group from block {}. Pos(x:{}, y:{}). Offset(x:{}, y:{}). Scale(x:{}, y:{})",
                    ins.name, ins.location.x, ins.location.y, self.offset.x, self.offset.y, self.scale_fact.x * ins.x_scale_factor,
                    self.scale_fact.y * ins.y_scale_factor);
                Ok(Objects::Group(
                    block
                        .entities
                        .iter()
                        .filter_map(|ent| {
                            ObjectsBuilder::new(ent, self.spline_step, self.px_per_mm)
                                .offsets(
                                    ins.location.x - block.base_point.x,
                                    ins.location.y - block.base_point.y,
                                )
                                .scaling(
                                    self.scale_fact.x * ins.x_scale_factor,
                                    self.scale_fact.y * ins.y_scale_factor,
                                )
                                .blocks(self.blocks)
                                .build()
                                .ok()
                        })
                        .collect(),
                ))
            }
            EntityType::Leader(leader) => {
                let ld: Leader = leader.into();

                Ok(Objects::Group(
                    ld.0.into_iter()
                        .map(|mut ln| {
                            ln.scale(self.scale_fact.x, self.scale_fact.y);

                            ln.x1 += self.offset.x;
                            ln.y1 -= self.offset.y;

                            ln.x2 += self.offset.x;
                            ln.y2 -= self.offset.y;

                            Objects::Line(ln)
                        })
                        .collect(),
                ))
            }
            EntityType::AttributeDefinition(attrib) => Ok({
                //need to look up the proper way to get the color for the Attrib
                let mut dtext = DTextBuilder::from_attrib(attrib)
                    .color(HexColor::from_u32(self.ent.common.color_24_bit as u32))
                    .build();

                dtext.scale(self.scale_fact.x, self.scale_fact.y);

                dtext.x += self.offset.x;
                dtext.y -= self.offset.y;

                Objects::DynamicText(dtext)
            }),
            _ => {
                //dbg!(&self.ent.specific);
                Err("Need to implement the rest of the entity types")
            }
        }
    }
}

impl From<&Objects> for Either<XMLElement, Vec<XMLElement>> {
    fn from(obj: &Objects) -> Self {
        match obj {
            Objects::Arc(arc) => Either::Left(arc.into()),
            Objects::Ellipse(ell) => Either::Left(ell.into()),
            Objects::Polygon(poly) => Either::Left(poly.into()),
            Objects::DynamicText(dtext) => Either::Left(dtext.into()),
            Objects::Text(txt) => Either::Left(txt.into()),
            Objects::Line(line) => Either::Left(line.into()),
            Objects::Group(block) => Either::Right(
                block
                    .iter()
                    //.flatten()
                    .filter_map(|obj| XMLElement::try_from(obj).ok())
                    .collect(),
            ),
        }
    }
}

impl TryFrom<&Objects> for XMLElement {
    type Error = &'static str; // add better error later

    fn try_from(obj: &Objects) -> Result<Self, Self::Error> {
        match obj {
            Objects::Arc(arc) => Ok(arc.into()),
            Objects::Ellipse(ell) => Ok(ell.into()),
            Objects::Polygon(poly) => Ok(poly.into()),
            Objects::DynamicText(dtext) => Ok(dtext.into()),
            Objects::Text(txt) => Ok(txt.into()),
            Objects::Line(line) => Ok(line.into()),
            Objects::Group(_) => Err("Unsupported"),
        }
    }
}

#[derive(Debug)]
pub struct Description {
    pub objects: Vec<Objects>,
}

impl ScaleEntity for Description {
    fn scale(&mut self, fact_x: f64, fact_y: f64) {
        self.objects
            .iter_mut()
            .for_each(|ob| ob.scale(fact_x, fact_y));
    }

    fn left_bound(&self) -> f64 {
        let lb = self.objects.iter().min_by(|ob1, ob2| {
            ob1.left_bound()
                .partial_cmp(&ob2.left_bound())
                .unwrap_or(std::cmp::Ordering::Greater)
        });

        if let Some(lb) = lb {
            lb.left_bound()
        } else {
            0.0
        }
    }

    fn right_bound(&self) -> f64 {
        let rb = self.objects.iter().max_by(|ob1, ob2| {
            ob1.right_bound()
                .partial_cmp(&ob2.right_bound())
                .unwrap_or(std::cmp::Ordering::Less)
        });

        if let Some(rb) = rb {
            rb.left_bound()
        } else {
            0.0
        }
    }

    fn top_bound(&self) -> f64 {
        let tb = self.objects.iter().min_by(|ob1, ob2| {
            ob1.top_bound()
                .partial_cmp(&ob2.top_bound())
                .unwrap_or(std::cmp::Ordering::Greater)
        });

        if let Some(tb) = tb {
            tb.top_bound()
        } else {
            0.0
        }
    }

    fn bot_bound(&self) -> f64 {
        let bb = self.objects.iter().max_by(|ob1, ob2| {
            ob1.bot_bound()
                .partial_cmp(&ob2.bot_bound())
                .unwrap_or(std::cmp::Ordering::Less)
        });

        if let Some(bb) = bb {
            bb.top_bound()
        } else {
            0.0
        }
    }
}

impl From<&Description> for XMLElement {
    fn from(desc: &Description) -> Self {
        let mut desc_xml = XMLElement::new("description");
        for obj in &desc.objects {
            if let Ok(elem) = XMLElement::try_from(obj) {
                desc_xml.add_child(elem);
            }
            for obj in obj.descendants() {
                if let Ok(elem) = XMLElement::try_from(obj) {
                    desc_xml.add_child(elem);
                }
            }
        }
        desc_xml
    }
}

/*impl TryFrom<Drawing> for Description {
    type Error = &'static str; //add better error later

    fn try_from(drw: Drawing) -> Result<Self, Self::Error> {
        drw.entities().filter_map(|ent| Objects::try_from(ent).ok()).collect();
    }
}*/
impl From<(&Drawing, u32, f64)> for Description {
    fn from((drw, spline_step, px_per_mm): (&Drawing, u32, f64)) -> Self {
        let _from_drw_span = span!(Level::TRACE, "Converting Drawing to Description");

        Self {
            objects: drw
                .entities()
                .filter_map(|ent| match &ent.specific {
                    EntityType::Insert(ins) => {
                        let block = find_block(drw, &ins.name)?;
                        let blocks: Vec<&Block> = drw.blocks().collect();
                        trace!(
                            "Creating Group from block {}. Pos(x:{}, y:{}). Scale(x:{}, y:{})",
                            ins.name,
                            ins.location.x,
                            ins.location.y,
                            ins.x_scale_factor,
                            ins.y_scale_factor
                        );
                        Some(Objects::Group(
                            block
                                .entities
                                .iter()
                                .filter_map(|ent| {
                                    ObjectsBuilder::new(ent, spline_step, px_per_mm)
                                        //very confused here, in one test file if I leave out the ins locations here it puts things in the
                                        //wrong location, and puts them in the correct location when I add the ins location in.
                                        //but in another file it's the opposite, not sure why the difference...
                                        .offsets(ins.location.x, ins.location.y)
                                        .scaling(ins.x_scale_factor, ins.y_scale_factor)
                                        .blocks(&blocks)
                                        .build()
                                        .ok()
                                })
                                .collect(),
                        ))
                    }
                    _ => ObjectsBuilder::new(ent, spline_step, px_per_mm).build().ok(),
                })
                .collect(),
        }
    }
}

//probably don't need to worry about this as they won't exist in the dxf...
/*pub struct Terminal {
    x: f64,
    y: f64,
    uuid: Uuid,
    name: String,
    orientation: TermOrient,
    //type?
    //  Generic
    //  Indoor Terminal Block
    //  External Terminal Block
}*/

#[derive(Debug)]
pub struct Names {
    names: Vec<Name>,
}

impl From<&Names> for XMLElement {
    fn from(nme: &Names) -> Self {
        let mut names_elmt = XMLElement::new("names");
        for name in &nme.names {
            let mut nm_elmt = XMLElement::new("name");
            nm_elmt.add_attribute("lang", &name.lang);
            nm_elmt.add_text(&name.value);
            names_elmt.add_child(nm_elmt);
        }
        names_elmt
    }
}

#[derive(Debug)]
pub struct Name {
    lang: String, //should this be an enum of language shorts at some point, maybe not worth it
    value: String,
}

#[derive(Debug)]
pub struct ElmtUuid {
    uuid: Uuid,
}

impl From<Uuid> for ElmtUuid {
    fn from(uuid: Uuid) -> Self {
        ElmtUuid { uuid }
    }
}

impl From<&ElmtUuid> for XMLElement {
    fn from(uuid: &ElmtUuid) -> Self {
        let mut uuid_xml = XMLElement::new("uuid");
        uuid_xml.add_attribute("uuid", format!("{{{}}}", uuid.uuid));
        uuid_xml
    }
}

//I need to check what these other item types are used for. I think it's unlikely
//I'll ever need for this tool, so it might be worth just hard coding the "element"
//string when writing out the XML, but for now I'll just comment out the other enum
//variants to suppress the clippy warnings.
#[derive(Debug)]
enum ItemType {
    Element = 1,
    /*ElementsCategory = 2,
    ElementsCollection = 4,
    ElementsContainer = 6,
    ElementsCollectionItem = 7,
    TitleBlockTemplate = 8,
    TitleBlockTemplatesCollection = 16,
    TitleBlockTemplatesCollectionItem = 24,
    Diagram = 32,
    Project = 64,
    All = 127,*/
}

impl Display for ItemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Element => "element",
                /*Self::ElementsCategory | Self::ElementsContainer | Self::ElementsCollectionItem =>
                    "elements category",
                Self::ElementsCollection => "element",
                Self::TitleBlockTemplate | Self::TitleBlockTemplatesCollectionItem =>
                    "title block template",
                Self::TitleBlockTemplatesCollection => "title block templates collection",
                Self::Diagram => "diagram",
                Self::Project => "project",
                Self::All => "All",*/
            }
        )
    }
}

#[derive(Debug)]
pub enum HAlignment {
    Left,
    Center,
    Right,
}

impl Display for HAlignment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Left => "AlignLeft",
                Self::Center => "AlignHCenter",
                Self::Right => "AlignRight",
            }
        )
    }
}

impl From<AttachmentPoint> for HAlignment {
    fn from(value: AttachmentPoint) -> Self {
        match value {
            AttachmentPoint::TopLeft
            | AttachmentPoint::MiddleLeft
            | AttachmentPoint::BottomLeft => HAlignment::Left,
            AttachmentPoint::TopCenter
            | AttachmentPoint::MiddleCenter
            | AttachmentPoint::BottomCenter => HAlignment::Center,
            AttachmentPoint::TopRight
            | AttachmentPoint::MiddleRight
            | AttachmentPoint::BottomRight => HAlignment::Right,
        }
    }
}

impl From<HorizontalTextJustification> for HAlignment {
    fn from(value: HorizontalTextJustification) -> Self {
        //https://ezdxf.readthedocs.io/en/stable/tutorials/text.html#tut-text
        match value {
            HorizontalTextJustification::Left => HAlignment::Left,
            HorizontalTextJustification::Center => HAlignment::Center,
            HorizontalTextJustification::Right => HAlignment::Right,

            //TODO: Handling the Aligned Middle and Fit alignments are a bit more complicated
            //for now I'll just default if it gets one of those we Alighn Left
            _ => HAlignment::Left,
        }
    }
}

#[derive(Debug)]
pub enum VAlignment {
    Top,
    Center,
    Bottom,
}

impl Display for VAlignment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Top => "AlignTop",
                Self::Center => "AlignVCenter",
                Self::Bottom => "AlignBottom",
            }
        )
    }
}

impl From<AttachmentPoint> for VAlignment {
    fn from(value: AttachmentPoint) -> Self {
        match value {
            AttachmentPoint::TopLeft | AttachmentPoint::TopCenter | AttachmentPoint::TopRight => {
                VAlignment::Top
            }
            AttachmentPoint::MiddleLeft
            | AttachmentPoint::MiddleCenter
            | AttachmentPoint::MiddleRight => VAlignment::Center,
            AttachmentPoint::BottomLeft
            | AttachmentPoint::BottomCenter
            | AttachmentPoint::BottomRight => VAlignment::Bottom,
        }
    }
}

impl From<VerticalTextJustification> for VAlignment {
    fn from(value: VerticalTextJustification) -> Self {
        //https://ezdxf.readthedocs.io/en/stable/tutorials/text.html#tut-text
        match value {
            VerticalTextJustification::Top => VAlignment::Top,
            VerticalTextJustification::Middle => VAlignment::Center,
            VerticalTextJustification::Baseline | VerticalTextJustification::Bottom => {
                VAlignment::Bottom
            }
        }
    }
}

#[derive(Debug)]
enum LineEnd {
    None,
    SimpleArrow,
    /*TriangleArrow,
    Circle,
    Diamond,*/
}

impl Display for LineEnd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::None => "none",
                Self::SimpleArrow => "simple",
                /*Self::TriangleArrow => "triangle",
                Self::Circle => "circle",
                Self::Diamond => "diamond",*/
            }
        )
    }
}

#[derive(Debug)]
enum LinkType {
    Simple,
    /*Master,
    Slave,
    NextReport,
    PrevReport,
    TermBlock,
    Thumbnail,*/
}

impl Display for LinkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Simple => "simple",
                /*Self::Master => "master",
                Self::Slave => "slave",
                Self::NextReport => "next_report",
                Self::PrevReport => "previous_report",
                Self::TermBlock => "terminal",
                Self::Thumbnail => "thumbnail",*/
            }
        )
    }
}

#[derive(Debug)]
pub struct ElemInfos {
    elem_info: Vec<ElemInfo>,
}

impl From<&ElemInfos> for XMLElement {
    fn from(elems: &ElemInfos) -> Self {
        let mut elems_xml = XMLElement::new("elementInformations");
        for elem in &elems.elem_info {
            elems_xml.add_child(elem.into());
        }

        elems_xml
    }
}

#[derive(Debug)]
pub struct ElemInfo {
    //there seems to be a list in the editor with the following values (per the XML)
    //  * supplier
    //  * description
    //  * machine_manufacturer_reference
    //  * manufacturer_reference
    //  * quantity
    //  * manufacturer
    //  * label
    //  * unity
    //  * plant
    //  * comment
    //  * designation
    // But can it only ever be these values? Might need to dig into the code. For now I'll use a string
    name: String,

    //I would assume show would be a bool...but instead of a true value I'm getting a "1"  in the XML
    //generated by the element editor. Maybe this means something else? I'll use an i32 for now
    show: i32,

    value: String,
}

impl From<&ElemInfo> for XMLElement {
    fn from(elem: &ElemInfo) -> Self {
        let mut elem_xml = XMLElement::new("elementInformation");
        elem_xml.add_attribute("show", elem.show);
        elem_xml.add_attribute("name", &elem.name);
        elem_xml.add_text(&elem.value);

        elem_xml
    }
}

#[inline]
pub fn two_dec(num: f64) -> f64 {
    (num * 100.0).round() / 100.0
}

//Should be the relevant Qt5 Code for the font string in Qt5...
//Might need to look it up for Qt6, since it appears to have changed
//and add in support for either or?

/*https://codebrowser.dev/qt5/qtbase/src/gui/text/qfont.cpp.html
/*!
    Returns a description of the font. The description is a
    comma-separated list of the attributes, perfectly suited for use
    in QSettings, and consists of the following:
    \list
    \li Font family
    \li Point size
    \li Pixel size
    \li Style hint
    \li Font weight
    \li Font style
    \li Underline
    \li Strike out
    \li Fixed pitch
    \li Always \e{0}
    \li Capitalization
    \li Letter spacing
    \li Word spacing
    \li Stretch
    \li Style strategy
    \li Font style (omitted when unavailable)
    \endlist
    \sa fromString()
 */
QString QFont::toString() const
{
    const QChar comma(QLatin1Char(','));
    QString fontDescription = family() + comma +
        QString::number(     pointSizeF()) + comma +
        QString::number(      pixelSize()) + comma +
        QString::number((int) styleHint()) + comma +
        QString::number(         weight()) + comma +
        QString::number((int)     style()) + comma +
        QString::number((int) underline()) + comma +
        QString::number((int) strikeOut()) + comma +
        QString::number((int)fixedPitch()) + comma +
        QString::number((int)   false);
    QString fontStyle = styleName();
    if (!fontStyle.isEmpty())
        fontDescription += comma + fontStyle;
    return fontDescription;
}
    */

#[derive(Debug)]
pub enum FontStyleHint {
    Helvetica,
    Times,
    Courier,
    OldEnglish,
    System,
    AnyStyle,
    Cursive,
    Monospace,
    Fantasy,
}

/*impl FontStyleHint {
    pub const SansSerif: FontStyleHint = FontStyleHint::Helvetica;
    pub const Serif: FontStyleHint = FontStyleHint::Times;
    pub const TypeWriter: FontStyleHint = FontStyleHint::Courier;
    pub const Decorative: FontStyleHint = FontStyleHint::OldEnglish;
}
*/

impl From<&FontStyleHint> for i32 {
    fn from(value: &FontStyleHint) -> Self {
        match value {
            FontStyleHint::Helvetica => 0,
            FontStyleHint::Times => 1,
            FontStyleHint::Courier => 2,
            FontStyleHint::OldEnglish => 3,
            FontStyleHint::System => 4,
            FontStyleHint::AnyStyle => 5,
            FontStyleHint::Cursive => 6,
            FontStyleHint::Monospace => 7,
            FontStyleHint::Fantasy => 8,
        }
    }
}

#[derive(Debug)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

impl From<&FontStyle> for i32 {
    fn from(value: &FontStyle) -> Self {
        match value {
            FontStyle::Normal => 0,
            FontStyle::Italic => 1,
            FontStyle::Oblique => 2,
        }
    }
}

#[derive(Debug)]
pub struct FontInfo {
    pub family: String,
    pub point_size: f64,
    pub pixel_size: i32,
    pub style_hint: FontStyleHint,
    pub weight: i32,
    pub style: FontStyle,
    pub underline: bool,
    pub strike_out: bool,
    pub fixed_pitch: bool,
    pub style_name: Option<String>,
}

impl Default for FontInfo {
    fn default() -> Self {
        //Might want to revisit these defaults
        //but I'll put something in for now
        Self {
            family: "osifont".into(),
            point_size: 12.0,
            pixel_size: i32::default(),
            style_hint: FontStyleHint::Helvetica,
            weight: i32::default(),
            style: FontStyle::Normal,
            underline: false,
            strike_out: false,
            fixed_pitch: false,
            style_name: None,
        }
    }
}

impl Display for FontInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{},{},{},{},{},{},{},{},{},0{}",
            self.family,
            self.point_size.round(),
            self.pixel_size,
            Into::<i32>::into(&self.style_hint),
            self.weight,
            Into::<i32>::into(&self.style),
            i32::from(self.underline),
            i32::from(self.strike_out),
            i32::from(self.fixed_pitch),
            if let Some(sn) = &self.style_name {
                format!(",{sn}")
            } else {
                String::new()
            },
        )
    }
}

#[derive(Debug)]
enum TextEntity<'a> {
    Text(&'a dxf::entities::Text),
    MText(&'a dxf::entities::MText),
    Attrib(&'a AttributeDefinition),
}
