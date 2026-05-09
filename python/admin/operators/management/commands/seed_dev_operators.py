"""
Management command: seed_dev_operators
=======================================

Creates the two canonical dev operator records used in local development and
CI smoke tests.  The command is idempotent — running it twice is safe.

Seeded operators
----------------
dev-tenant1@example.test  role='admin'   tenant_id=00000000-0000-0000-0000-000000000001
dev-super@example.test    role='super'   tenant_id=NULL  (workspace-wide, BYPASSRLS)

Usage
-----
    uv run python manage.py seed_dev_operators

Sprint-4 follow-ups
-------------------
- Replace this command with a real self-service provisioning flow (SSO → Operator).
- Document the actual onboarding runbook for production operators.
"""

import uuid

from django.core.management.base import BaseCommand

from operators.models import Operator

_DEV_TENANT_UUID = uuid.UUID("00000000-0000-0000-0000-000000000001")

_SEED_OPERATORS = [
    {
        "email": "dev-tenant1@example.test",
        "role": Operator.ROLE_ADMIN,
        "tenant_id": _DEV_TENANT_UUID,
        "is_active": True,
    },
    {
        "email": "dev-super@example.test",
        "role": Operator.ROLE_SUPER,
        "tenant_id": None,
        "is_active": True,
    },
]


class Command(BaseCommand):
    help = "Seed dev operator records (idempotent)."

    def handle(self, *args, **options):
        created_count = 0
        for spec in _SEED_OPERATORS:
            obj, created = Operator.objects.update_or_create(
                email=spec["email"],
                defaults={
                    "role": spec["role"],
                    "tenant_id": spec["tenant_id"],
                    "is_active": spec["is_active"],
                },
            )
            if created:
                created_count += 1
                self.stdout.write(self.style.SUCCESS(f"  Created: {obj}"))
            else:
                self.stdout.write(f"  Already exists (updated): {obj}")

        self.stdout.write(
            self.style.SUCCESS(
                f"seed_dev_operators complete. {created_count} new, "
                f"{len(_SEED_OPERATORS) - created_count} updated."
            )
        )
