-- 0007: permission to bill a customer beyond their credit limit (manager approval flow).
INSERT INTO permissions (code, description) VALUES ('sales.credit_override', 'Credit override (sales)');
-- Owner and manager have every permission except user and backup management.
INSERT INTO role_permissions (role_id, permission_code) VALUES
  ('01920000-0000-7000-8000-000000000001', 'sales.credit_override'),
  ('01920000-0000-7000-8000-000000000002', 'sales.credit_override');
