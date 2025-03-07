--
-- Name: periodic_table; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.periodic_table (
    "AtomicNumber" integer NOT NULL,
    "Element" text,
    "Symbol" text,
    "AtomicMass" numeric,
    "NumberOfNeutrons" integer,
    "NumberOfProtons" integer,
    "NumberOfElectrons" integer,
    "Period" integer,
    "Group" integer,
    "Phase" text,
    "Radioactive" boolean,
    "Natural" boolean,
    "Metal" boolean,
    "Nonmetal" boolean,
    "Metalloid" boolean,
    "Type" text,
    "AtomicRadius" numeric,
    "Electronegativity" numeric,
    "FirstIonization" numeric,
    "Density" numeric,
    "MeltingPoint" numeric,
    "BoilingPoint" numeric,
    "NumberOfIsotopes" integer,
    "Discoverer" text,
    "Year" integer,
    "SpecificHeat" numeric,
    "NumberOfShells" integer,
    "NumberOfValence" integer
);


--
-- Data for Name: periodic_table; Type: TABLE DATA; Schema: public; Owner: -
--
INSERT INTO public.periodic_table ("AtomicNumber", "Element", "Symbol", "AtomicMass", "NumberOfNeutrons", "NumberOfProtons", "NumberOfElectrons", "Period", "Group", "Phase", "Radioactive", "Natural", "Metal", "Nonmetal", "Metalloid", "Type", "AtomicRadius", "Electronegativity", "FirstIonization", "Density", "MeltingPoint", "BoilingPoint", "NumberOfIsotopes", "Discoverer", "Year", "SpecificHeat", "NumberOfShells", "NumberOfValence") VALUES
(1, 'Hydrogen', 'H', 1.007, 0, 1, 1, 1, 1, 'gas', NULL, true, NULL, true, NULL, 'Nonmetal', 0.79, 2.2, 13.5984, 0.0000899, 14.175, 20.28, 3, 'Cavendish', 1766, 14.304, 1, 1);


ALTER TABLE ONLY public.periodic_table
    ADD CONSTRAINT periodic_table_pkey PRIMARY KEY ("AtomicNumber");

