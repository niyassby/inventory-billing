-- 0008: clearing account for goods received before the supplier's bill (GRN → bill).
-- A goods receipt debits Inventory and credits this account; the bill then debits it instead of
-- Inventory, so the Inventory account always equals stock value.
INSERT INTO accounts (id, code, name, account_type, subtype, is_system, created_at, updated_at)
VALUES ('01920000-0000-7000-8001-000000002050', '2050', 'Goods Received Not Invoiced', 'liability', 'other', 1,
        '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
